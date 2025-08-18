use client::fuse::SimpleFS;
use fuser;
use fuser::MountOption;
use std::process::Command;
use std::fs;


fn main() {
    let mountpoint = std::env::args().nth(1).expect("Usage: fuse-test <MOUNTPOINT>");

    // Provo a smontare se era già montato
    let _ = Command::new("fusermount")
        .args(&["-u", &mountpoint])
        .status();

    // Ricreo la cartella (se era rimasta "zombie" la elimino e la rifaccio pulita)
    let _ = fs::remove_dir_all(&mountpoint);
    let _ = fs::create_dir_all(&mountpoint);

    fuser::mount2(SimpleFS, mountpoint, &[MountOption::RO]).unwrap();
}
