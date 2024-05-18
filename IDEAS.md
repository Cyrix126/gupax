# IDEAS for the future of Gupaxx


**Theses are only ideas, everything here is still to be decided and only thoughts for now.**
Some ideas could be done in a matter of hours, some could take months.

## More Decentralized
### Integrate a Monero Node
If we want Gupaxx to help user mine in the most decentralized way, we should offer them to run a monero node.
This would be optional and would check if the requirement are fulfilled before enabling the button to do so.
### Synchronize source code repository on p2p network
Github is proprietary. If Gupaxx aims to be free, it should not be only available on this platform and we should explore options to get github free.  
We can use [Radicle](https://radicle.xyz/) to get Gupaxx on a p2p collaboration stack.  
The code, issues and PR could be synchronized with Github.  

## More User friendly
### Website
Build a website like [gupax.io](https://gupax.io) to have a more user frendly presentation and installation method.  
Having a website, we can detect the architecture and os of the visitor and give him the right archive to download.
### Generated wallet
If Gupaxx could create a wallet and put the primary address in p2pool tab automaticcly, it would remove a manual step for the user.  
It could be an option to ask at first start.  
The user could access this wallet on the same computer with the official GUI wallet.
### Auto register to XvB
If Gupaxx could register the user automaticcly to the raffle, it would remove a manual step for the user.  
It could be an option to ask at first start.  
### Setup Guide
At first start, a guide could ask the user what it intends to do with Gupaxx (create node, create wallet, use xmrig-proxy, participate in raffle...) and do the setup for him and show him what it must do manually. An option to skip this guide would be present for advanced users.

## Supporting more environnements
### Packaging
Add repository/AUR for Gupaxx and a status of packaging distro/version on the README
### Minimum requirement
Add on README a table with minimum hardware/software requirements.
### Add more target
Gupaxx could add support for linux arm64 since p2pool and xmrig can compile on this target. 

## More Powerful
### Optimization for xmrig
#### Add automatic options
On linux, we can activate 1GB pages after detecting cpu flags. We can also add cpu affinity option.
#### Manual optimizations
On the XMRig tab, inform users about manual optimizations that Gupaxx can't control. For example, disabling hyper-threading in BIOS is recommended.
### CLI for Algorithm
For advanced users, a CLI could be made to use the algorithm without a GUI
It would allow the user to do automation and installation on headless environment and save a few HR from the Gupaxx process.
This CLI would still offer a way to the user to pass options to xmrig and to start his p2pool server or to use another one already running.  
#### Integrate XMRig-Proxy
The algorithm of distribution of HR can't control HR outside of his instance.
It must estimate external HR, which can be approximative.
If a user control multiples miners, it could connect all of them to a xmrig-proxy instance.
Gupaxx could offer this xmrig-instance and control it like it was a normal xmrig instance.  
OR
#### Watch Stratum Data instead of estimate.
Right now, the algorithm estimate the eHR with the estimation made by the p2pool instance which is calculating from passed shares.
The algorithm could instead watch the stats from the stratum server, which is exact but would take into account only miners which are pointed to it.
Miners using the cli xmrig could point their miners to the p2pool instance of Gupaxx, or have an identical option if they are using the CLI of the algorithm.
The algorithm would still check the estimation made by the p2pool instance of Gupaxx and warn the user if it seems there is too much difference between the data of the stratum server and the one of p2pool. It could prevent the user to forget to configure a miner to the stratum p2pool.


## Trust-less
### Reproducible builds
To remove necessary trust, binairies released should have the same checksum if recompiled without code change.
See [this](https://reproducible-builds.org).
### Release changes notes preview
Show the summuray of what will change between releases before updating to newer release.
### Donation transparency
So that user can see how much is given to this project and make their own opinion of if enough donations have been given or not, the history of donation should be made visible with the viewkey available.  
