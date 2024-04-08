# v1.0.0
Stable release

## Changes
### Internal
use latest OS for github CI  
bump deps  
remove unused CI actions  

## Fixes
fix https://github.com/Cyrix126/gupaxx/issues/1  

## Bundled Versions
* [`P2Pool v3.10`](https://github.com/SChernykh/p2pool/releases/tag/v3.10)  
* [`XMRig v6.21.1`](https://github.com/xmrig/xmrig/releases/tag/v6.21.1)  

# v0.1.10
Fix release for beta version.  
This version is only made for testing purposes and have feedback.  

## Changes
## Fixes
fix https://github.com/Cyrix126/gupaxx/issues/2 compile statically rustls-tls  
invalid test semver version.

## Bundled Versions
* [`P2Pool v3.10`](https://github.com/SChernykh/p2pool/releases/tag/v3.10)  
* [`XMRig v6.21.1`](https://github.com/xmrig/xmrig/releases/tag/v6.21.1)  

# v0.1.9
Fix release for beta version.  
This version is only made for testing purposes and have feedback.  

## Changes
### Internal
do not stop completely XvB process if stats can not be retrieved but retry every ten seconds and reload if they are retrieved again.  
Bump version  

## Fixes
Do not crash at startup if path of P2Pool executable is not valid and P2Pool is in auto start  

## Bundled Versions
* [`P2Pool v3.10`](https://github.com/SChernykh/p2pool/releases/tag/v3.10)  
* [`XMRig v6.21.1`](https://github.com/xmrig/xmrig/releases/tag/v6.21.1)  

# v0.1.8
Fix release for beta version.  
This version is only made for testing purposes and have feedback.  

## Changes
### Internal
Better manage fail of nodes from XvB  
## Fixes
Algorithm time too short under some conditions.  
Countdown too short under some conditions  
Request API stuck after some time  
Average local HR sent not saved  

## Bundled Versions
* [`P2Pool v3.10`](https://github.com/SChernykh/p2pool/releases/tag/v3.10)  
* [`XMRig v6.21.1`](https://github.com/xmrig/xmrig/releases/tag/v6.21.1)  

# v0.1.7
Fix release for beta version.  
This version is only made for testing purposes and have feedback.

## Changes
### User interface
Round type consider 1h average HR with 20% margin.
Remove fake AppImage

### Internal
Bump deps versions  
Better automatization for releases.  

### Documentation
Documentation of new directories in source code.  
Rework README with fresh screenshots, tutorial video and better help.

## Fixes
Given time was not subtracted from countdown when needed.  
Duplicate help message on input token

## Bundled Versions
* [`P2Pool v3.10`](https://github.com/SChernykh/p2pool/releases/tag/v3.10)  
* [`XMRig v6.21.1`](https://github.com/xmrig/xmrig/releases/tag/v6.21.1)  

# v0.1.6
Fix release for beta version.
This version is only made for testing purposes and have feedbacks.

## Changes
### User interface
Indicator with countdown for algorithm.  
Hero mode button active on next decision of algorithm without restart.  
Add info if algorithm decision is made with hero mode selected.  
Text on hover improvements for token input.  
Better displaying info about HR relative to algorithm on console output  
Add info if algorithm is waiting for XMRig average HR.  
### Internal
Use HTTP client default retry  
Bump deps versions  
#### XvB process
Immediately start algorithm when possible without delay.  
Will retrieve public and private stats just before algorithm rerun, so decision is based on last data.  
Algorithm takes longest average HR of XMRig depending on what's available (instead of depending of the number of run of the algorithm).  
#### Manage lost connection of XvB nodes
Continue XvB partially if XvB nodes fails instead of stopping.  
Make XMRig go back to P2Pool if needed after XvB nodes fail.  
Check continuously if XvB nodes came online after fail.  
Auto reload XvB process if XvB nodes came online.  
#### P2pool
Retrieve current shares as soon as p2pool process is synced.  

### Code Organization
Separate XvB process into submodules.  
Simplify code for XvB process.  
Put tests into own file.  
Update test to take into account margin on XvB side.  

## Fixes
Winner was not recognized.  
Did not take into account scale of sent sidechain P2Pool HR.  
Last hour average HR sent kept only one sample.  
Multiple instance of algorithm ran in parallel under some conditions.  
XMRig config was updated when not needed, even for 0 seconds.  
Calculation of time needed to send minimum HR for round type was sending all spared HR less outside XvB HR instead of just minimum HR for round type less oHR.  
Calculation of current round type was only looking if value was more than minimum required when it should look if value is more or equal (very few chances to have exactly equal HR but was noticed with the units test).  

## Bundled Versions
* [`P2Pool v3.10`](https://github.com/SChernykh/p2pool/releases/tag/v3.10)  
* [`XMRig v6.21.1`](https://github.com/xmrig/xmrig/releases/tag/v6.21.1)  

# v0.1.5
Fix release for beta version.
This version is only made for testing purposes and have feedbacks.

## Changes
update dependencies of UI
replace old HTTP client

## Fixes
fix formatting HR algorithm
fix private round calculation

## Bundled Versions
* [`P2Pool v3.10`](https://github.com/SChernykh/p2pool/releases/tag/v3.10)
* [`XMRig v6.21.1`](https://github.com/xmrig/xmrig/releases/tag/v6.21.1)

# v0.1.4
Fix release for beta version.
This version is only made for testing purposes and have feedbacks.

## Changes
new logo
algorithm wait for xmrig first value, takes 10s value at first start, 1m value at second start, 15m value at third start.

## Fixes
fix detection of p2pool eHR
fix private round type stats
fix name gupax tab to gupaxx

## Bundled Versions
* [`P2Pool v3.10`](https://github.com/SChernykh/p2pool/releases/tag/v3.10)
* [`XMRig v6.21.1`](https://github.com/xmrig/xmrig/releases/tag/v6.21.1)

# v0.1.3
Fix release for beta version.
This version is only made for testing purposes and have feedbacks.

## Changes
take into account outside HR

## Fixes
downgrade to xmrig 6.21.1 to solve xmrig stats showing only after 15m

## Bundled Versions
* [`P2Pool v3.10`](https://github.com/SChernykh/p2pool/releases/tag/v3.10)
* [`XMRig v6.21.1`](https://github.com/xmrig/xmrig/releases/tag/v6.21.1)

# v0.1.2
Fix release for beta version.
This version is only made for testing purposes and have feedbacks.

## Changes

## Fixes
Persist current shares value
fix script for release

## Bundled Versions
* [`P2Pool v3.10`](https://github.com/SChernykh/p2pool/releases/tag/v3.10)
* [`XMRig v6.21.2`](https://github.com/xmrig/xmrig/releases/tag/v6.21.2)


# v0.1.1
Fix release for beta version.
This version is only made for testing purposes and have feedbacks.

## Changes
Current Shares appears on P2pool column of Submenu process in Status Tab.
Parse the current shares from the status command of p2pool instead of an estimation based on shares found and time.

## Fixes
XvB algorithm now gets the number of shares instead of 0.

## Bundled Versions
* [`P2Pool v3.10`](https://github.com/SChernykh/p2pool/releases/tag/v3.10)
* [`XMRig v6.21.2`](https://github.com/xmrig/xmrig/releases/tag/v6.21.2)


# v0.1.0
First beta release of Gupaxx.  
This version is only made for testing purposes and have feedbacks.

## Changes
See [DIFFERENCES.md](DIFFERENCES.md)

## Fixes
See [DIFFERENCES.md](DIFFERENCES.md)

## Bundled Versions
* [`P2Pool v3.10`](https://github.com/SChernykh/p2pool/releases/tag/v3.10)
* [`XMRig v6.21.2`](https://github.com/xmrig/xmrig/releases/tag/v6.21.2)
