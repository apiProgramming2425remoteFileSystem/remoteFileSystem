use project::fs_model;
use project::fs_model::node::FileSystem;

fn create_file_system_with_structure() -> FileSystem {
    let mut fs = FileSystem::new();
    fs.make_dir("/", "home").unwrap();
    fs.change_dir("/home").unwrap();
    fs.make_dir(".", "user").unwrap();
    fs.change_dir("./user").unwrap();
    fs.make_file(".", "file.txt").unwrap();
    fs.make_file(".", "file1.txt").unwrap();
    fs.make_dir("..", "user1").unwrap();
    fs.change_dir("../user1").unwrap();
    fs.make_file(".", "file.txt").unwrap();
    fs.make_link("/home", "link_user", "/home/user").unwrap();
    fs
}

fn main() {
    let mut fs = create_file_system_with_structure();
    // mettiti in ascolto per le api
}
