use gifski;
use std::io;
use std::num;

error_chain! {
    types {
        Error, ErrorKind, ResultExt, BinResult;
    }
    foreign_links {
        GifSki(gifski::Error);
        Io(io::Error);
        Num(num::ParseIntError);
    }
}
