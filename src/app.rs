use std::{
    fs::{self, File},
    io::{BufRead, BufReader},
    path::Path,
    process::ExitCode,
};

use crate::{config::Config, table::Table};

pub fn list(config: &Config) -> ExitCode {
    let Some(ref nix_profile) = config.nix_profile else {
        eprintln!("Nix not installed");
        return ExitCode::FAILURE;
    };

    let appdir = config
        .resolve_symlink(nix_profile.join("share/applications"))
        .and_then(fs::read_dir)
        .expect("No applications found");

    let mut table = Table::new();
    table.add_header(String::from("ID"));
    table.add_header(String::from("NAME"));
    table.add_header(String::from("COMMAND"));
    table.add_header(String::from("COMMENT"));

    for entry in appdir {
        let entry = entry
            .and_then(|x| config.resolve_symlink(x.path()))
            .expect("Error occurred while finding applications");
        let Some(desktop) = DesktopFile::parse_file(&entry) else { continue; };

        let id: String = entry.file_stem().unwrap().to_str().unwrap().to_string();

        table.add_row(vec![
            id,
            desktop.name,
            desktop.exec,
            desktop.comment.unwrap_or(String::new()),
        ])
    }

    table.print();

    ExitCode::FAILURE
}

struct DesktopFile {
    name: String,
    exec: String,
    comment: Option<String>,
}

impl DesktopFile {
    fn parse_file(path: impl AsRef<Path>) -> Option<Self> {
        let file = File::open(path).ok()?;
        let reader = BufReader::new(file);

        let mut name = None;
        let mut exec = None;
        let mut comm = None;
        for line in reader.lines() {
            let line = line.ok()?;

            if let Some(val) = line.strip_prefix("Exec=") {
                exec = Some(val.trim().to_string());
            } else if let Some(val) = line.strip_prefix("Name=") {
                name = Some(val.trim().to_string());
            } else if let Some(val) = line.strip_prefix("Comment=") {
                comm = Some(val.trim().to_string());
            } else if line.starts_with("NoDisplay=true") || line.starts_with("Terminal=true") {
                return None;
            } else if let Some(val) = line.strip_prefix("Categories=") {
                if val
                    .trim()
                    .split(';')
                    .any(|category| category == "ConsoleOnly")
                {
                    return None;
                }
            }
        }

        Some(Self {
            name: name?,
            exec: exec?,
            comment: comm,
        })
    }
}
