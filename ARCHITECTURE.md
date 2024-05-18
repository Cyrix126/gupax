# Gupaxx ARCHITECTURE

This document explains how the source code is organized. Everything differing from [Gupax](https://github.com/hinto-janai/gupax) is described here. Things that do not change are not present.

## Structure
| File/Folder  | Purpose |
|--------------|---------|
|main.rs| launch the app
|inits.rs| launch the threads if auto, including XvB
|miscs.rs| useful functions
|app| directory with everything related to displaying the UI
|app/keys.rs| handle keys input
|app/mod.rs| define App struct, used by egui
|app/eframe_impl.rs| first entry to the UI
|disk/| Code for writing to disk: `state.toml/node.toml/pool.toml`; This holds the structs for the [State] struct
|helper| The "helper" thread that runs for the entire duration Gupax is alive. All the processing that needs to be done without blocking the main GUI thread runs here, including everything related to handling P2Pool/XMRig/XvB
|helper/xvb| All related thread XvB code
|helper/xvb/mod.rs| XvB thread and principal loop, checks and triggers, gluing every other code of this directory.
|helper/xvb/algorithm.rs| Algorithm logic with calculations and actions
|helper/xvb/nodes.rs| Manage connection of XvB nodes
|helper/xvb/rounds.rs| struct for Rounds with printing and detecting of current round.
|helper/xvb/public\|private_stats| struct to retrieve public and private stats with request
|component| Gupaxx related features, like updates and nodes


## Technical differences of column XMRig in Status Tab process sub menu with upstream Gupax

Status of process for Xmrig use for some information an image of data when the process started.
The node of xmrig in upstream can not change without a restart of the process.In this fork, the node used by xmrig needs to be updated without restart (using the config HTTP API of xmrig).
So Gupaxx need to refresh the value of status tab submenu process for xmrig where before the values could not change without a restart of the process.
The field node from ImgXmrig needs to be moved to PubXvbApi. This value must be updated by xmrig at start and by XvB process at runtime.

## Updates

A new option in Gupaxx tab advanced will enable bundled updates.
The binary included of gupaxx will have default value for bundled updates depending if it is coming from standalone or bundle release.

Updates from Gupaxx will do the following differently from upstream:
- check if using bundled or standalone with state. Update only Gupaxx binary if the latter or xmrig and p2pool from bundle version if the former.
- prevent user to run updates twice without restart.
- ask the user to restart Gupaxx
- do not verify if file p2pool or xmrig exist. (so that the update can create them).
