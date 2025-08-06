use wasmlink::marshal_struct;

marshal_struct! {
    pub struct MessageLogArgs {
        level: u32,
        line: u32,
        column: u32,
        msg: Vec<u8>,
    }
}
