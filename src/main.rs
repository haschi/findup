use std::{
    collections::HashMap,
    error::Error,
    fs::{self, ReadDir},
    path::{Path, PathBuf},
};

use clap::{Parser, ValueEnum};
use sha2::{Sha256, Digest};

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    ///
    #[arg(short, long, value_enum, default_value_t = Output::Human)]
    output: Output,


    #[arg(long, default_value_t = true)]
    human: bool,

    #[arg(short, long, conflicts_with = "human")]
    machine: bool,

    #[arg(short = 'd', long, default_value_t = 1)]
    max_depth: u32,

    /// Die Verzeichnisse, in denen nach doppelten Dateien gesucht wird.
    ///
    /// Wenn kein Verzeichnis angegeben ist, wird das aktuelle Verzeichnis
    /// durchsucht. Wenn ein Verzeichnis angegeben ist, wird dieses
    /// Verzeichnis durchsucht.
    // #[arg(default_values_t = ["./"])]
    #[arg(value_name = "DIRS", value_hint = clap::ValueHint::DirPath)]
    directories: Vec<PathBuf>,
}

#[derive(Copy, Clone, PartialEq, ValueEnum, Debug)]
enum Output {
    Human,
    Machine,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    // Das Programm arbeitet in verschiedenen Modi:
    // 1. Wenn kein Verzeichnis angegeben ist, durchsuche das aktuelle
    // Verzeichnis rekursiv nach Duplikaten.
    // 2. Wenn genau ein Verzeichnis angegeben ist, durchsuche dieses
    // Verzeichnis nach Duplikaten
    // 3. Wenn zwei oder mehr Verzeichnisse angegeben sind, suche nach
    // Duplikaten vom ersten Verzeichnis in den nachfolgenden
    // Verzeichnissen

    match args.directories.len() {
        0 => {
            let cwd = std::env::current_dir()?;
            mode1(&args, cwd)?;
        }
        1 => mode1(&args, &args.directories[0])?,
        _ => {
            todo!();
        }
    }

    Ok(())
}

enum Entry {
    File { path: PathBuf, len: u64 },
    Error { path: PathBuf, err: std::io::Error },
}

struct Walker {
    iterator_stack: Vec<ReadDir>,
}

impl Walker {
    fn new<P>(path: P) -> Result<Walker, std::io::Error>
    where
        P: AsRef<Path>,
    {
        let mut walker = Walker {
            iterator_stack: Vec::new(),
        };
        walker.iterator_stack.push(fs::read_dir(path)?);
        Ok(walker)
    }
}

#[derive(Clone)]
enum Same {
    SameSize(Vec<PathBuf>),
    Checksums(ChecksumMap)
}

impl Same {
    fn print(&self, args: &Args) {
        match args.output {
            Output::Human => {    
                match self {
                    Same::SameSize(paths) => {
                        println!("{}", paths[0].display());
                        for duplicate in &paths[1..] {
                            println!("    {}", duplicate.display())
                        }
                    },
                    Same::Checksums(map) => {
                        for (_, paths) in map {
                            println!("{}", paths[0].display());
                            for duplicate in &paths[1..] {
                                println!("    {}", duplicate.display())
                            }
                        }
                    }
                }
            },
            Output::Machine => {
                match self {
                    Same::SameSize(paths) => {
                        for duplicate in &paths[1..] {
                            println!("{}", duplicate.display())
                        }
                    },

                    Same::Checksums(map) => {
                        for (_, paths) in map {
                            for duplicate in &paths[1..] {
                                println!("{}", duplicate.display())
                            }
                        }
                    }
                }
            }
        }
    }
}

type ChecksumMap = HashMap<[u8; 32], Vec<PathBuf>>;


type Duplicates = HashMap<u64, Same>;

/// Der Iterator liefert Entries für reguläre Dateien.
///
/// Wenn ein Zugriff auf die Datei möglich ist und die Länge der Datei
/// bestimmt werden kann, liefert der Aufruf dieser Funktion eine
/// [Entry::File] Variante mit dem Pfad und der Größe der Datei.
/// Wenn der nächste Verzeichniseintrag nicht gelesen werden kann, liefert
/// die Funktion eine [Entry::Error] Variante mit dem Pfad zum Verzeichniseintrag
/// und dem Fehler.
///
/// Einträge
impl Iterator for Walker {
    type Item = Entry;

    fn next(&mut self) -> Option<Self::Item> {
        let len = self.iterator_stack.len();
        if len == 0 {
            return None;
        }

        let current_iterator = &mut self.iterator_stack[len - 1];

        if let Some(result) = current_iterator.next() {
            match result {
                Ok(entry) => {
                    let path = entry.path();
                    let md = entry.metadata().unwrap();
                    let len = md.len();

                    match entry.file_type() {
                        Ok(typ) => {
                            if typ.is_dir() {
                                return match fs::read_dir(&path) {
                                    Ok(neu) => {
                                        self.iterator_stack.push(neu);
                                        self.next()
                                    }
                                    Err(err) => Some(Entry::Error { path, err }),
                                };
                            } else if typ.is_file() {
                                Some(Entry::File { path, len })
                            } else {                                
                                self.next()
                            }
                        }
                        Err(err) => Some(Entry::Error { path, err }),
                    }
                }
                Err(err) => Some(Entry::Error {
                    path: PathBuf::new(),
                    err,
                }),
            }
        } else {
            self.iterator_stack.pop();
            self.next()
        }
    }
}

// Akkumulator für Iterator::fold mit dem Dateien gleicher Größe
// in einer Hashmap zusammengefasst werden.
//
// TODO: Fehlerbehandlung. Damit bei der späteren Ausgabe Fehler
// berücksichtigt werden können, müssen diese hier verarbeitet
// werden.
//
// Ideen:
//   1. Die Fehler zählen: Duplicates ist eine Struktur mit einem
//     Feld zum zählen der Fehler. Der Zähler wird in der 
//     Zusammenfassung ausgeben. Die Zusammenfassung enthält zum 
//     Beispiel die Anzahl der Duplikate, Anzahl an Bytes, die durch
//     das Löschen der Duplikate freigegeben werden könne und die
//     Anzahl der Dateien / Verzeichnisse, auf die nicht zugegriffen 
//     werden kann.
//   2. Fehler werden innerhalb dieser Funktion in die Standardfehler
//     Ausgabe geschrieben (und dann vergessen).
fn group_by_len(mut map: Duplicates, entry: Entry) -> Duplicates {
    match entry {
        Entry::File { path, len } => {
            if let Same::SameSize(bucket) = map.entry(len).or_insert_with(|| Same::SameSize(Vec::new())) {
                bucket.push(path)
            } else {
                unreachable!()
            }            
            map
        }
        // Kann auch ein Fehler sein. Dann Result Error 
        _ => map,
    }
}

fn print_result(args: &Args, result: Duplicates) {
    for (key, same) in result {
        same.print(args)
    }
}

fn same_size_to_checksums(same: &Same) -> Same {
    if let Same::SameSize(size) = same {
        if size.len() > 1 {
            let mut map = ChecksumMap::new();

            for path in size {
                // TODO: was soll passieren, wenn die Datei nicht gelesen werden kann?
                if let Ok(content) = fs::read(path) {
                    let hash: [u8; 32] = Sha256::digest(content).into();
                    let entry = map.entry(hash).or_insert_with(|| Vec::new());
                    entry.push(path.to_owned())
                }
            }
            Same::Checksums(map)
        } else {
            same.clone()
        }

    } else {
        same.clone()
    }
}

fn same_size_to_checksums2((size, same): (u64, Same)) -> (u64, Same) {
    if let Same::SameSize(paths) = &same {
        if paths.len() > 1 {
            let mut map = ChecksumMap::new();

            for path in paths {
                // TODO: was soll passieren, wenn die Datei nicht gelesen werden kann?
                if let Ok(content) = fs::read(path) {
                    let hash: [u8; 32] = Sha256::digest(content).into();
                    let entry = map.entry(hash).or_insert_with(|| Vec::new());
                    entry.push(path.to_owned())
                }
            }
            (size, Same::Checksums(map))
        } else {
            (size, same.clone())
        }

    } else {
        (size, same.clone())
    }
}

// Nur ein Verzeichnis nach Duplikaten durchsuchen
fn mode1<P>(args: &Args, path: P) -> Result<(), std::io::Error>
where
    P: AsRef<Path>,
{
    // 1. Pass: File size
    let walker = Walker::new(path)?;
    let pass1 = walker.fold(Duplicates::new(), group_by_len);    
    let pass2: Duplicates = pass1.into_iter().map(same_size_to_checksums2).collect();

    print_result(args, pass2);

    Ok(())
}

// fn traverse2<P>(first: P) -> Result<(), std::io::Error>
// where
//     P: AsRef<Path>,
// {
//     let walker = Walker::new(first)?;

//     let pass1 = walker.fold(HashMap::new(), group_by_len);

//     println!("{:#?}", pass1);
//     Ok(())
// }