The help looks like this:

```
$ findup --help
Sucht nach doppelten Dateien

Usage: findup [OPTIONS] [DIRS]...

Arguments:
  [DIRS]...
          Die Verzeichnisse, in denen nach doppelten Dateien gesucht wird.
          
          Wenn kein Verzeichnis angegeben ist, wird das aktuelle Verzeichnis durchsucht. Wenn ein Verzeichnis angegeben ist, wird dieses Verzeichnis durchsucht.

Options:
  -o, --output <OUTPUT>
          [default: human]
          [possible values: human, machine]

      --human
          

  -m, --machine
          

  -d, --max-depth <MAX_DEPTH>
          [default: 1]

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version

```

Find duplicates in current working directory:

```
$ findup
[CWD]/hello.txt
Unique files: 1. 0 files waste 0 Bytes.

```