use clap::Parser;

mod compress {}

#[derive(Parser)]
pub enum Arguments {
    Compress,
}

pub fn main() -> Result<(), xshell::Error> {
    let args = Arguments::parse();
    let sh = xshell::Shell::new()?;

    match args {
        Arguments::Compress => {
            let mut archive_builder = oaf::builder::ArchiveBuilder::new();

            let files = sh.read_dir("testdir")?;
            for file_path in files {
                let file = sh.read_binary_file(&file_path)?;
                let file_name = file_path.file_name().unwrap().to_string_lossy();

                archive_builder.push_entry(oaf::Signature::File, &file_name, &[], &file);
            }

            let archive = archive_builder.finish();

            let archive = oaf::Archive::new(&archive).unwrap();

            for entry in archive.iter() {
                let data_str = std::str::from_utf8(entry.data()).unwrap();

                println!("\n{}\n{}", entry.name(), data_str);
            }
        }
    }

    Ok(())
}
