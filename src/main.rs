use std::{
    collections::HashMap,
    error::Error,
    fs::{self, ReadDir},
    path::{Path, PathBuf},
};

use clap::{Parser, ValueEnum};
use colored::Colorize;
use sha2::{Digest, Sha256};

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
    Checksums(ChecksumMap),
}

impl Same {
    fn print(&self, args: &Args) {
        match args.output {
            Output::Human => match self {
                Same::SameSize(paths) => {
                    println!("{}", paths[0].display());
                    for duplicate in &paths[1..] {
                        println!("    {}", duplicate.display())
                    }
                }
                Same::Checksums(map) => {
                    for (_, paths) in map {
                        println!("{}", paths[0].display());
                        for duplicate in &paths[1..] {
                            println!("    {}", duplicate.display())
                        }
                    }
                }
            },
            Output::Machine => match self {
                Same::SameSize(paths) => {
                    for duplicate in &paths[1..] {
                        println!("{}", duplicate.display())
                    }
                }

                Same::Checksums(map) => {
                    for (_, paths) in map {
                        for duplicate in &paths[1..] {
                            println!("{}", duplicate.display())
                        }
                    }
                }
            },
        }
    }
}

type ChecksumMap = HashMap<[u8; 32], Vec<PathBuf>>;

struct Duplicates(HashMap<u64, Same>);

impl Duplicates {
    fn new() -> Duplicates {
        Duplicates(HashMap::new())
    }

    fn summarize(&self) -> Summary {
        let mut s = Summary::default();

        for (size, same) in self.0.iter() {
            match same {
                Same::SameSize(paths) => {
                    let files = paths.len() as u64;
                    s.files += files;
                    s.candidates += files - 1;
                    s.bytes += size * (files - 1)
                }
                Same::Checksums(map) => {
                    for (_hash, paths) in map {
                        let files = paths.len() as u64;
                        s.files += files;
                        s.candidates += files - 1;
                        s.bytes += size * (files - 1)
                    }
                }
            }
        }

        s
    }
}

#[derive(Default, Debug)]
struct Summary {
    files: u64,
    candidates: u64,
    bytes: u64,
}

impl Summary {
    fn print(&self, args: &Args) {
        if args.output == Output::Human {
            println!(
                "Unique files: {}. {} files waste {} Bytes.",
                format!("{}", self.files - self.candidates).green(), 
                format!("{}", self.candidates).red(), 
                self.bytes)
    
        }
    }
}

impl FromIterator<(u64, Same)> for Duplicates {
    fn from_iter<T: IntoIterator<Item = (u64, Same)>>(iter: T) -> Self {
        let map: HashMap<u64, Same> = iter.into_iter().collect();
        Duplicates(map)
    }
}

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
            if let Same::SameSize(bucket) = map
                .0
                .entry(len)
                .or_insert_with(|| Same::SameSize(Vec::new()))
            {
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
use std::iter;

fn as_path_iterator(item: &Same) -> impl Iterator<Item = &Vec<PathBuf>> + '_ {
   let result =  match item {
        Same::Checksums(cs) => {
            Box::new(cs.values()) as Box<dyn Iterator<Item = &Vec<PathBuf>>>
            // todo!()
        }

        Same::SameSize(s) => {
            Box::new(iter::once(s)) as Box<dyn Iterator<Item = &Vec<PathBuf>>>
            // todo!()
            // vec![].into_iter()
        }
    };

    result
    // if let Same::Checksums(cs) = item {
    //     let a  = cs.values().cloned();
    //     a
    // } else let Same::SameSize(s) = item {
    //     let b = iter::once(s);
    //     b
    // }
}

fn print_result(args: &Args, result: &Duplicates) {

    // Generiere eine Liste mit allen Gruppen von Dateien mit
    // gleicher Größe bzw. gleicher Prüfsumme. Das Kriterium
    // geht bei dieser Operation verloren.
    let mut x: Vec<&Vec<PathBuf>> = result.0.values().flat_map(as_path_iterator).collect();

    x.sort_by(|a, b| {
        (**a)[0].cmp(&(**b)[0])
    });

    for candidates in x {

        match args.output {
            Output::Human => {
                println!("{}", candidates[0].display());
                for duplicate in &candidates[1..] {
                    println!("    {}", duplicate.display())
                }
            }
            Output::Machine => {
                if candidates.len() > 1 {
                    for duplicate in &candidates[1..] {
                        println!("{}", duplicate.display())
                    }
                }
            }
        }
    }

    // for (key, same) in &result.0 {
    //     same.print(args)
    // }

    let summary = result.summarize();
    summary.print(args);
}

fn same_size_to_checksums((size, same): (u64, Same)) -> (u64, Same) {
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
    let pass2: Duplicates = pass1.0.into_iter().map(same_size_to_checksums).collect();

    print_result(args, &pass2);

    Ok(())
}
