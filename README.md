# Crucible Runtime

This is the WASM runtime component of the Crucible game engine.

The runtime is designed off one core idea: rather than try to link a bunch of plugin modules dynamically—weathering all the ABI issues and runtime inefficiencies associated with that strategy—simply load one giant module implementing the user's entire game and deduplicate the module's bytecode using a content-addressable file system. This does mean that server developers will have to compile their servers from scratch but that experience can always be simplified with good tooling.

## Research

We can gain access to the relocation section of a binary by removing the `--gc-sections` flag and adding the `-r` flag to the linker. Here's an example of how to do that:

In `my_linker.sh`...

```shell
args=("$@")
for ((i=0; i<"${#args[@]}"; ++i)); do
    case ${args[i]} in
        --gc-sections) unset args[i]; unset args[i+1]; break;;
    esac
done

$(rustc --print=sysroot)/lib/rustlib/$(rustc -vV | awk '/host:/ {print $2}')/bin/rust-lld "${args[@]}" -r
```

In `start.sh`...

```shell
cargo rustc -p crucible-guest --target wasm32-wasi -- -C linker=./my_linker.sh -C linker-flavor=wasm-ld
```

This ensures that the wasm linker doesn't omit the relocation sections when creating the final WASM module. These sections are formatted as described in [this document](https://github.com/WebAssembly/tool-conventions/blob/main/Linking.md).

Here's all the sections we have access to:

```
".debug_abbrev"
".debug_info"
".debug_str"
".debug_pubnames"
".debug_pubtypes"
".debug_line"
".debug_ranges"
"linking"
"reloc.CODE"
"reloc.DATA"
"reloc..debug_info"
"reloc..debug_pubnames"
"reloc..debug_pubtypes"
"reloc..debug_line"
"reloc..debug_ranges"
"name"
"producers"
"target_features"
```
