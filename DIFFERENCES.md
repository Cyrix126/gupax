# Differences with upstream [Gupax](https://github.com/hinto-janai/gupax)

## Integration of the XvB Raffle

A new fancy tab to apply an algorithm of distribution of HR to xmrig (see [NOTES_ALGORITHM](NOTES_ALGORITHMS)) with your token from XvB.  
This tab also includes a console output to let you track if everything is working and what are the decision of the algorithm, and show you personal stats from XvB.

A new column in Status Tab to see public stats from the raffle.

## Removed functionality

Updates by tor. The version of the crate used was outdated, plagued with security concerns and bloated the binary.  
It was only for updates.  
If you want Gupaxx to update by tor, you can torify it when launching.

## Technical Debt

All dependencies are upgraded to last possible version, even when there is a breaking change (code of Gupaxx is modified for that).

## Bugfixes (visuals and performances)

The rendering of Tabs has been modified so that the minimum stated size of the window is capable to show everything. In Upstream middles panels often overlap on the bottom.

The rendering of the benchmark table and of console outputs were calculating every line at the same time. Now it only renders what you see. It is a significant improvement for your processor, and you can feel the difference if it is not very powerful.

Updates from Gupaxx does not retrieve xmrig and p2pool from upstream anymore, but use versions in the bundled version. This modification prevent bad surprise (see #3).

It also allows advanced users to use your their own version of p2pool and xmrig.The standalone version of Gupaxx will not replace them.

## Security

With the upgrade of dependencies, cargo audit show no warnings instead of 5 vulnerabilities and 4 allowed warnings for Gupax. 
