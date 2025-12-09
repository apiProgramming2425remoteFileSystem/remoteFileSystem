use actix_web::{HttpResponse, Responder, delete, get, post, put, web};
use tracing::{Level, instrument};

use crate::db::DB;
use crate::db::get_expiration_time;
use crate::models::*;
use crate::storage::*;

const APP_V1_BASE_URL: &str = "/api/v1";

// This function configures all routes for your module
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope(APP_V1_BASE_URL)
            .service(list_path)
            .service(get_file_content)
            .service(write_file)
            .service(make_directory)
            .service(delete_item)
            .service(rename)
            .service(resolve_child)
            .service(get_attributes)
            .service(set_attributes)
            .service(get_permissions)
            .service(get_stats)
            .service(login)
            .service(logout)
            .service(set_x_attributes)
            .service(get_x_attributes)
            .service(list_x_attributes)
            .service(delete_x_attributes),
    );
}

#[get("/list/{path}")]
#[instrument(skip(fs ), ret(level = Level::DEBUG))]
async fn list_path(fs: web::Data<FileSystem>, user: AuthenticatedUser, path: web::Path<String>) -> impl Responder {
    let path = path.into_inner();
    let Some(item) = fs.find(&path) else {
        return HttpResponse::NotFound().json(String::from("Path not found"));
    };
    let Some(children_nodes) = item.get_children() else {
        return HttpResponse::BadRequest().json(String::from("Path isn't a directory"));
    };
    let children: Vec<SerializableFSItem> = children_nodes
        .iter()
        .map(|child| SerializableFSItem::new(child))
        .collect();
    HttpResponse::Ok().json(children)
}

#[get("/files/{path}")]
#[instrument(skip(fs ), ret(level = Level::DEBUG))]
async fn get_file_content(fs: web::Data<FileSystem>, user: AuthenticatedUser, path: web::Path<String>) -> impl Responder {
     

    let path = path.into_inner();

    match fs.read_file(&path, 0) {
        Ok(content) => HttpResponse::Ok().json(SerializableFileContent::new(&content)),
        Err(e) => HttpResponse::InternalServerError().json(format!("Failed to read file: {}", e)),
    }
}

#[put("/files/{path}")]
#[instrument(skip(fs ), ret(level = Level::DEBUG))]
async fn write_file(
    fs: web::Data<FileSystem>,
    user: AuthenticatedUser, 
    path: web::Path<String>,
    json: web::Json<WriteFileRequest>,
) -> impl Responder {
     

    let path = path.into_inner();
    let offset = json.offset();
    let Ok(data) = json.data() else {
        return HttpResponse::BadRequest().body("Invalid base64 data");
    };

    return match fs.write_file(&path, &data, offset) {
        Ok(_) => HttpResponse::Ok().body("Write successful"),
        Err(e) => HttpResponse::InternalServerError().body(format!("Write failed: {}", e)),
    };
}

#[post("/mkdir/{path}")]
#[instrument(skip(fs ), ret(level = Level::DEBUG))]
async fn make_directory(fs: web::Data<FileSystem>, user: AuthenticatedUser, path: web::Path<String>) -> impl Responder {
     

    let path = path.into_inner();
    let (parent, name) = match path.rsplit_once('/') {
        Some((p, n)) => (if p.is_empty() { "/" } else { p }, n),
        None => return HttpResponse::BadRequest().body("Invalid path"),
    };
    if let Err(e) = fs.make_dir(parent, name) {
        tracing::error!("mkdir failed: {}", e);
        return HttpResponse::InternalServerError().body(format!("Mkdir failed: {}", e));
    }

    let attributes = match fs.get_attributes(path.as_str()) {
        Ok(a) => a,
        Err(e) => {
            tracing::error!("mldir failed: {}", e);
            return HttpResponse::InternalServerError().body(format!("Mkdir failed: {}", e));
        }
    };

    HttpResponse::Ok().json(attributes)
}

#[delete("/files/{path}")]
#[instrument(skip(fs ), ret(level = Level::DEBUG))]
async fn delete_item(fs: web::Data<FileSystem>, user: AuthenticatedUser, path: web::Path<String>) -> impl Responder {
     

    let path = path.into_inner();
    match fs.delete(path.as_str()) {
        Ok(_) => HttpResponse::Ok().body("Successful deletion!"),
        Err(s) => HttpResponse::BadRequest().body(format!("Delete failed: {}", s)),
    }
}

/* Inutile se lookup = getAttr */
#[get("/resolve/{path}")]
#[instrument(skip(fs ), ret(level = Level::DEBUG))]
async fn resolve_child(fs: web::Data<FileSystem>, user: AuthenticatedUser, path: web::Path<String>) -> impl Responder {
     

    let path = path.into_inner();
    match fs.get_attributes(path.as_str()) {
        Ok(attributes) => HttpResponse::Ok().json(attributes),
        Err(e) => HttpResponse::InternalServerError().json(e.to_string()),
    }
}

#[get("/attributes/{path}")]
#[instrument(skip(fs), ret(level = Level::DEBUG))]
async fn get_attributes(fs: web::Data<FileSystem>, user: AuthenticatedUser, path: web::Path<String>) -> impl Responder {
    let path = path.into_inner();
    match fs.get_attributes(path.as_str()) {
        Ok(attributes) => HttpResponse::Ok().json(attributes),
        Err(e) => {
            tracing::error!("{}", e.to_string());
            return HttpResponse::NotFound().json(e.to_string());
        }
    }
}

#[put("/attributes/{path}")]
#[instrument(skip(fs), ret(level = Level::DEBUG))]
async fn set_attributes(fs: web::Data<FileSystem>, user: AuthenticatedUser, path: web::Path<String>, json: web::Json<SetAttrRequest>) -> impl Responder {
    let path = path.into_inner();

    let uid = json.uid();
    let gid = json.gid();
    let new_attributes = json.setattr();

    match fs.set_attributes(path.as_str(), uid, gid, new_attributes) {
        Ok(attributes) => HttpResponse::Ok().json(attributes),
        Err(e) => {
            tracing::error!("{}", e.to_string());
            return HttpResponse::InternalServerError().json(e.to_string());
        }
    }
}

#[get("/permissions/{path}")]
#[instrument(skip(fs ), ret(level = Level::DEBUG))]
async fn get_permissions(fs: web::Data<FileSystem>, user: AuthenticatedUser, path: web::Path<String>) -> impl Responder {
     

    let path = path.into_inner();

    match fs.get_permissions(path.as_str()) {
        Ok(permissions) => HttpResponse::Ok().json(permissions),
        Err(e) => {
            tracing::error!("{}", e.to_string());
            return HttpResponse::InternalServerError().json(e.to_string());
        }
    }
}

#[get("/stats/{path}")]
async fn get_stats(fs: web::Data<FileSystem>, user: AuthenticatedUser, path: web::Path<String>) -> impl Responder {
     

    let path = path.into_inner();

    match fs.get_fs_stats(path.as_str()) {
        Ok(stats) => HttpResponse::Ok().json(stats),
        Err(e) => {
            tracing::error!("{}", e.to_string());
            return HttpResponse::InternalServerError().json(e.to_string());
        }
    }
}

#[put("/rename")]
#[instrument(skip(fs ), ret(level = Level::DEBUG))]
async fn rename(fs: web::Data<FileSystem>, user: AuthenticatedUser, json: web::Json<RenameRequest>) -> impl Responder {
     
    
    let old_path = json.old_path();
    let new_path = json.new_path();

    match fs.rename(&old_path, &new_path) {
        Ok(()) => HttpResponse::Ok().body("Successful renaming!"),
        Err(_) => HttpResponse::BadRequest().body("Something went wrong"),
    }
}

/* AUTHENTICATION MANAGEMENT */
#[post("/login")]
#[instrument(skip(pool ), ret(level = Level::DEBUG))]
async fn login(pool: web::Data<DB>, form: web::Json<LoginBody>) -> impl Responder {
        let result = pool.authenticate_user(&form.username, &form.password).await; 
        
        match result{
            Ok(token) =>{
                match token{
                    Some(t) => return HttpResponse::Ok().json(Token::new(t)),
                    None => return HttpResponse::Unauthorized().finish(),
                }
            },
            Err(e) => return HttpResponse::InternalServerError().json(e.to_string()),
        };
}

#[post("/logout")]
#[instrument(skip(pool), ret(level = Level::DEBUG))]
pub async fn logout(pool: web::Data<DB>, user: AuthenticatedUser) -> impl Responder {
    match pool.insert_revoked_token(&user).await {
        Ok(_) => return HttpResponse::Ok().body("Logged out"),
        Err(e) => return HttpResponse::InternalServerError().json(e.to_string()),
    };
}

/* XATTRIBUTES MANEGEMENT */
#[put("/xattributes/{path}/names/{name}")]
#[instrument(ret(level = Level::DEBUG))]
#[instrument(skip(pool), ret(level = Level::DEBUG))]
async fn set_x_attributes(pool: web::Data<DB>, user: AuthenticatedUser, path: web::Path<String>, name: web::Path<String>, json: web::Json<Xattributes>) -> impl Responder{
    let path = path.into_inner();
    let name = name.into_inner();

    match pool.set_x_attributes(&path, &name, &json.get()).await {
        Ok(()) => HttpResponse::Ok().finish(),
        Err(e) => HttpResponse::InternalServerError().json(e.to_string())
    }
}

#[get("/xattributes/{path}/names/{name}")]
#[instrument(ret(level = Level::DEBUG))]
#[instrument(skip(pool), ret(level = Level::DEBUG))]
async fn get_x_attributes(pool: web::Data<DB>, user: AuthenticatedUser, name: web::Path<String>, path: web::Path<String>) -> impl Responder {
    let path = path.into_inner();
    let name = name.into_inner();

    let result = pool.get_x_attributes(&path, &name).await;
    match result {
        Ok(option) => {
            match option{
                Some(attr) => HttpResponse::Ok().json(attr),
                None => HttpResponse::NotFound().finish(),
            }
        },
        Err(e) => {
            tracing::error!("{}", e.to_string());
            return HttpResponse::InternalServerError().json(e.to_string());
        }
    }
}

#[get("/xattributes/{path}/names")]
#[instrument(ret(level = Level::DEBUG))]
#[instrument(skip(pool), ret(level = Level::DEBUG))]
async fn list_x_attributes(pool: web::Data<DB>, user: AuthenticatedUser, path: web::Path<String>) -> impl Responder {
    let path = path.into_inner();

    let result = pool.list_x_attributes(&path).await;
    match result {
        Ok(option) => {
            match option{
                Some(names) => HttpResponse::Ok().json(names),
                None => HttpResponse::NotFound().finish(),
            }
        },
        Err(e) => {
            tracing::error!("{}", e.to_string());
            return HttpResponse::InternalServerError().json(e.to_string());
        }
    }
}


#[delete("/xattributes/{path}/names/{name}")]
#[instrument(ret(level = Level::DEBUG))]
#[instrument(skip(pool), ret(level = Level::DEBUG))]
async fn delete_x_attributes(pool: web::Data<DB>, user: AuthenticatedUser, path: web::Path<String>, name: web::Path<String>) -> impl Responder{
    let path = path.into_inner();
    let name = name.into_inner();

    match pool.remove_x_attributes(&path, &name).await {
        Ok(()) => HttpResponse::Ok().finish(),
        Err(e) => HttpResponse::InternalServerError().json(e.to_string())
    }
}