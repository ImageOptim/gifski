use gifski;
use std::io;

error_chain! {
    types {
        Error, ErrorKind, ResultExt, BinResult;
    }
    foreign_links {
        GifSki(gifski::Error);
        Io(io::Error);
    }
}
