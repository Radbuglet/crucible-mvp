# Crucible Runtime

This is the WASM runtime component of the Crucible game engine.

The runtime is designed off one core idea: rather than try to link a bunch of plugin modules dynamically—weathering all the ABI issues and runtime inefficiencies associated with that strategy—simply load one giant module implementing the user's entire game and deduplicate the module's bytecode using a content-addressable file system. This does mean that server developers will have to compile their servers from scratch but that experience can always be simplified with good tooling.
