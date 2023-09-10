pub mod webmote {
    pub mod proto {
        include!(concat!(env!("OUT_DIR"), "/webmote.rs"));
    }
}