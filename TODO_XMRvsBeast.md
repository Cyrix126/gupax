# TODO

# Bounty

[XvB Bounty](https://bounties.monero.social/posts/105)

*These todos are not all part of the bounty.*

- [x] upgrade deps
- [x] separate logic in smaller modules
- [x] new tab XvB
  - [x] logo
  - [x] link to website
  - [x] link and message hovering explaining registration and needs to read the rules.
  - [x] token input
  - [x] hero checkbox
    - [x] without restart of xvb (next decision of algo)
  - [x] log section
    - [x] state of XvB process
    - [x] selected XvB node
    - [x] algorithm decisions info
    - [x] timestamp
  - [x] private stats
      - [x] from XvB API (fails, average 1h and 24h)
      - [x] round type in
      - [x] win or loose
  - [x] indicator of mining round and time left.
  - [x] new process for XvB
    - [x] update preferred XvB node based on ping and backup 
      - [x] fix: xmrig will not do anything if node is not responding. Need to parse output of xmrig for error and update nodes.
    - [x] status process XvB
    - [x] status process XMRig node in real time.
    - [x] public information from [API](https://xmrvsbeast.com/p2pool/stats)
    - [x] stop, start, restart buttons
    - [x] button to autostart
    - [x] distribute hashrate conforming to the algorithm.
      - [x] check every 10 minutes average Xmrig HR of last 15 minutes
      - [x] fast startup (check 10s first decision then 1m second decision)
      - [x] take into account outside HR
        - [x] mining on p2pool
        - [x] mining on XvB
      - [x] ask Xmrig to mine on p2pool
        - [x] generate token for xmrig
        - [x] enable xmrig with remote access control
      - [x] check if at least a share in pplns Window
      - [x] calculate spared HR
      - [x] calculate time to be spared
        - [x] with hero option
        - [x] without hero option, to give minimum to be in most accessible round type
      - [x] sleep 10mn less time to spare then ask Xmrig to mine on XvB node
    - [x] output log to console in XvB tab
- [x] fix some overlapping from upstream
- [x] edit metadata of project
  - [x] adapt README for XvB 
    - [x] beta release
    - [x] stable release
  - [x] video tutorial to set up XvB Tab
  - [x] adapt doc for new code
  - [x] cargo package metadata
  - [x] pgp signatures
  - [x] rename reference of gupax name to gupaxx
  - [x] replace AppImage with a desktop file
- [x] fix clippy
- [x] optimizations
  - [x] benchmarks table render only what is visible
  - [x] console output render only what is visible
    - [x] remove arti
    - [x] replace hyper with reqwest
- [x] better organize new code
- [x] merge commits from upstream
- [x] tests for new function 
  - [x] time calculated by algorithm 
- [x] better automatization with utils for releases
- [x] pre-release
  - [x] feedback
- [x] release
