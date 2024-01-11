This is a command-line tool to check which groups of a [start.gg](https://start.gg/) event can still result in 3-way ties.

# Installation

1. (Skip this step if you're not on Windows.) If you're on Windows, you'll first need to download and install [Visual Studio](https://visualstudio.microsoft.com/vs/) (the Community edition should work). On the “Workloads” screen of the installer, make sure “Desktop development with C++” is selected. (Note that [Visual Studio Code](https://code.visualstudio.com/) is not the same thing as Visual Studio. You need VS, not VS Code.)
2. Install Rust:
    * On Windows, download and run [rustup-init.exe](https://win.rustup.rs/) and follow its instructions.
    * On other platforms, please see [the Rust website](https://www.rust-lang.org/tools/install) for instructions.
3. Open a command line:
    * On Windows, right-click the Start button, then click “Terminal”, “Windows PowerShell”, or “Command Prompt”.
    * On other platforms, look for an app named “Terminal” or similar.
4. In the command line, run the following command. Depending on your computer, this may take a while.

    ```
    cargo install --git=https://github.com/midoshouse/startgg-3way-ties --branch=main
    ```

# Usage

First, you will need a start.gg personal access token. You can obtain one by clicking on your profile picture in the lower left corner of [start.gg](https://start.gg/), selecting “Developer Settings”, then “Create new token”, giving it a description, and finally clicking the icon to copy it. Make sure to paste it somewhere before copying the event slug in the next step.

You will also need the “slug” of the event to analyze. In start.gg's terminology, an event is a part of a tournament, and the slug is that event's URL with the `https://www.start.gg/` removed.

For example, if your token is `abc123` and your event is at <https://www.start.gg/tournament/ocarina-of-time-randomizer-standard-tournament-season-7-challenge-cup/event/challenge-cup-season-7>, you'll want to run the following command:

```sh
startgg-3way-ties abc123 tournament/ocarina-of-time-randomizer-standard-tournament-season-7-challenge-cup/event/challenge-cup-season-7
```

The tool will list all of the groups it finds and whether a 3-way tie is guaranteed, possible (but not guaranteed), or impossible. If a 3-way tie is possible/guaranteed, it also lists all possible sets of scores the group members can still have. (Note that this list may contain duplicates.)
