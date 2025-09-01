mod encode;
mod format;
mod utils;

pub use self::{
    encode::{SplitModuleArgs, SplitModuleResult, split_module},
    format::WasmallArchive,
};
