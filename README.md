# osu-scrobbler

An osu! Last.fm and ListenBrainz scrobbler. This scrobbler only works for gameplay!

## Setup

1.  Download the latest release binary for your operating system [here](https://github.com/flazepe/osu-scrobbler/releases/latest).

2.  Create a folder called `osu-scrobbler` (or anything really) in your system and move the downloaded binary file there.

3.  In the same folder, create a file called `config.toml` with [this](https://github.com/flazepe/osu-scrobbler/blob/master/config.example.toml) as the content.

> [!TIP]
> The `config.toml` file can also be placed anywhere and its path can be supplied through the `OSU_SCROBBLER_CONFIG_PATH` environment variable while executing the binary.

4.  Get your Last.fm API credentials [here](https://www.last.fm/api/account/create) (or [here](https://www.last.fm/api/accounts) if you already have one).

    -   For ListenBrainz users, head [here](https://listenbrainz.org/profile/) to get your user token.

5.  Edit the configuration values accordingly.

    -   Most notably, your osu! user ID and scrobbler credentials.

6.  Execute the binary in the terminal and the scrobbler should start.

> [!TIP]
> If you're on Windows or macOS, you can also double click the binary file to launch the scrobbler.

## Configuration

Refer [here](https://github.com/flazepe/osu-scrobbler/wiki/Configuration) for more details about the configuration properties.

This fork also adds `repeat_cooldown_mins` under `[scrobbler]`: once a song is scrobbled, playing it again won't scrobble it until it hasn't been played for this many minutes. Every repeated play restarts the timer, so retrying the same map over and over only scrobbles it once. Default: `0` (scrobble every play).

## Autostart

You can configure the scrobbler to automatically start on startup. Refer [here](https://github.com/flazepe/osu-scrobbler/wiki/Autostart) for the necessary steps.

### Windows (without the wiki)

#### Option 1: Run in Background (Hidden)
This method runs the scrobbler silently without a visible terminal window.

1. Ensure `launch_hidden.vbs` is in the same folder as `osu-scrobbler.exe`.
2. Right-click `launch_hidden.vbs` and select **Create shortcut**.
3. Press `Win + R` on your keyboard, type `shell:startup`, and press Enter.
4. Move the newly created shortcut into the Startup folder that opens.

#### Option 2: Run with Terminal (Visible)
This method keeps the terminal window open, which is useful if you want to monitor logs.

Follow the same steps as Option 1, but create the shortcut for `osu-scrobbler.exe` instead.
