# Human readable output

```
$ findup
[CWD]/duplicate1.txt
    [CWD]/sample1.txt
[CWD]/sample2.txt
[CWD]/sample3.txt
Unique files: 3. 1 files waste 6 Bytes.

```

# Machine readable output

List all duplicated files in current directory one file per line. No 
summary will be written.

```trycmd
$ findup --output machine
[CWD]/sample1.txt

```

You can pass machine readable output to `xargs` command:

Mit trycmd können derzeit keine Pipes implementiert werden.

```ignore
$ findup --output machine | xargs -L 1 echo
[CWD]/duplicate1.txt
```