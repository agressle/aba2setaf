# aba2setaf
A tool for converting an ABA framework to a SETAF.

## 1 Installation

Requires the rust toolchain (c.f. https://www.rust-lang.org and https://rustup.rs). Run 'cargo build –release' to build. 

## 2 Usage 

Usage: aba2setaf --instance <FILE> --destination <FILE>

Options:
  * -i, --instance `<FILE>`\
     A file that contains the encoding of the instance in the format used by the [ICCMA competition](https://argumentationcompetition.org/2025/rules.html).
  * -d, --destination `<FILE>`\
     The file to write the resulting SETAF to. [description file format](https://github.com/agressle/GSAFSolver#description-file-format).  
  * -o, --overwrite\
     When provided, the destination file will be overwritten if already present.
  * -a, --asp\
     When provided, the ASP format will be used for resulting SETAF. Otherwise, the DIMCAS-like format is used, see also: [instance file format](https://github.com/agressle/GSAFSolver#instance-file-format).
