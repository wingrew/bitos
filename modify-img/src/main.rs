// 本文件是为了在本地测试时向文件镜像中写入文件
extern crate fatfs;
extern crate clap;
use clap::{App, Arg};
use std::fs::{read_dir, File};
use std::io::{Read, Write};
fn main() -> std::io::Result<()>{
    // 解析命令行参数
    let matches = App::new("EasyFileSystem packer")
        .arg(
            Arg::with_name("source")
                .short("s")
                .long("source")
                .takes_value(true)
                .help("Executable source dir(with backslash)"),
        )
        .arg(
            Arg::with_name("target")
                .short("t")
                .long("target")
                .takes_value(true)
                .help("Executable target dir(with backslash)"),
        )
        .get_matches();
    let src_path = matches.value_of("source").unwrap();
    let target_path = matches.value_of("target").unwrap();
    println!("src_path = {}\ntarget_path = {}", src_path, target_path);
    let img = std::fs::OpenOptions::new().read(true).write(true)
        .open(format!("{}{}", target_path, "sdcard.img"));
    let img_file = img?;
    let fs = fatfs::FileSystem::new(img_file, fatfs::FsOptions::new())?;
    // 获取根目录
    let root_dir = fs.root_dir();
    let apps: Vec<_> = read_dir(src_path)
    .unwrap()
    .into_iter()
    .map(|dir_entry| {
        let name_with_ext = dir_entry.unwrap().file_name().into_string().unwrap();           
        name_with_ext
    })
    .collect();
    // 遍历文件夹下的所有文件
    for app in apps {
        // load app data from host file system
        println!("{:?}", app);
        let mut host_file = File::open(format!("{}{}", src_path, app)).unwrap();
        let mut all_data: Vec<u8> = Vec::new();
        host_file.read_to_end(&mut all_data).unwrap();
        // create a file in easy-fs
        let mut file = root_dir.create_file(app.as_str()).expect("Failed to create file");
        // write data to easy-fs
        file.write_all(all_data.as_slice()).expect("Failed to write to file");
    }
    println!("文件写入成功！");
    Ok(())
}
