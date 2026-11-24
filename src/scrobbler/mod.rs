mod last_fm;
mod listenbrainz;

use crate::{
    config::{Config, ScrobblerConfig},
    logger::Logger,
    scores::Score,
    scrobbler::{last_fm::LastfmScrobbler, listenbrainz::ListenBrainzScrobbler},
    utils::{get_osu_pid, handle_redirects, validate_scrobble},
};
use anyhow::{Context, Result};
use chrono::{DateTime, FixedOffset, Utc};
use colored::Colorize;
use reqwest::blocking::Client;
use std::{
    collections::HashMap,
    sync::LazyLock,
    thread::sleep,
    time::{Duration, SystemTime},
};

static REQWEST: LazyLock<Client> = LazyLock::new(Client::new);

#[derive(Debug)]
pub struct Scrobbler {
    config: ScrobblerConfig,
    config_modified: SystemTime,
    config_reload_result: Result<()>,
    last_fm: Option<LastfmScrobbler>,
    listenbrainz: Option<ListenBrainzScrobbler>,
    recent_score: Option<Score>,
    cooldown_secs: u64,
    // Songs scrobbled recently, with the time they were last played (used for `repeat_cooldown_mins`)
    recently_scrobbled: HashMap<String, DateTime<FixedOffset>>,
}

impl Scrobbler {
    pub fn new() -> Result<Self> {
        Logger::success("Scrobbler", format!("Starting... ({})", format!("v{}", env!("CARGO_PKG_VERSION")).bright_blue()), false);

        let (config, config_modified) = Config::init()?;
        let last_fm = if let Some(c) = config.last_fm { Some(LastfmScrobbler::new(c)?) } else { None };
        let listenbrainz = if let Some(c) = config.listenbrainz { Some(ListenBrainzScrobbler::new(c)?) } else { None };

        Ok(Self {
            config: config.scrobbler,
            config_modified,
            config_reload_result: Ok(()),
            last_fm,
            listenbrainz,
            recent_score: None,
            cooldown_secs: 0,
            recently_scrobbled: HashMap::new(),
        })
    }

    pub fn start(&mut self) -> Result<()> {
        self.recent_score = Score::get_user_recent(&self.config)?;

        Logger::success("Scrobbler", "Started!", false);

        loop {
            self.cooldown_secs = 0;

            self.reload_config();

            if get_osu_pid().is_some() {
                self.poll();
            }

            self.cooldown_secs += 5;

            sleep(Duration::from_secs(self.cooldown_secs));
        }
    }

    fn reload_config(&mut self) {
        match self.config.reload(&mut self.config_modified).context("Could not reload config file.") {
            Ok(new_recent_score) => {
                if let Some(new_recent_score) = new_recent_score {
                    self.recent_score = Some(new_recent_score);
                }

                self.config_reload_result = Ok(());
            },
            Err(new_error) => {
                let new_error_string = format!("{new_error:?}");

                if self.config_reload_result.as_ref().err().is_none_or(|error| format!("{error:?}") != new_error_string) {
                    Logger::error("Config", new_error_string, false);
                }

                self.config_reload_result = Err(new_error);
            },
        }
    }

    fn poll(&mut self) {
        match Score::get_user_recent(&self.config) {
            Ok(score) => {
                let Some(score) = score else { return };
                self.scrobble(&score);
                self.recent_score = Some(score);
            },
            Err(error) => {
                Logger::error("Scrobbler", error, false);
                self.cooldown_secs += 10;
            },
        }
    }

    fn scrobble(&mut self, score: &Score) {
        if self.recent_score.as_ref().is_some_and(|recent_score| recent_score.ended_at == score.ended_at) {
            return;
        }

        if !score.passed {
            let started_at = score.started_at.as_ref().and_then(|started_at| DateTime::parse_from_rfc3339(started_at).ok());
            let ended_at = DateTime::parse_from_rfc3339(&score.ended_at).ok();
            let delta = ended_at.and_then(|ended_at| started_at.map(|started_at| (ended_at - started_at).as_seconds_f64()));

            if let Some(delta) = delta {
                let rate = score
                    .mods
                    .iter()
                    .find(|score_mod| score_mod.acronym == "DT" || score_mod.acronym == "NC")
                    .and_then(|score_mod| score_mod.settings.as_ref().map(|settings| settings.speed_change.unwrap_or(1.5)))
                    .unwrap_or(1.);
                let hit_length = score.beatmap.hit_length as f64 / rate;

                // A valid scrobble should be half of the beatmap's hit length or 4 minutes, whichever occurs earlier
                // This might go through if the user paused, took a long break, and continued (just to fail some time after)
                let is_valid_scrobble = delta >= hit_length / 2. || delta >= 60. * 4.;

                if !is_valid_scrobble {
                    return;
                }
            }
        }

        let (artist_romanized, artist_original) = (&score.beatmapset.artist, &score.beatmapset.artist_unicode);
        let (title_romanized, title_original) = (&score.beatmapset.title, &score.beatmapset.title_unicode);

        let mut artist = artist_romanized;
        let mut title = title_romanized;

        if self.config.use_original_metadata {
            artist = artist_original;
            title = title_original;
        }

        let played_at = DateTime::parse_from_rfc3339(&score.ended_at).unwrap_or_else(|_| Utc::now().fixed_offset());
        let song = format!("{} - {}", score.beatmapset.artist.to_lowercase(), score.beatmapset.title.to_lowercase());

        if self.is_repeat(&song, played_at) {
            Logger::warn(
                "Scrobbler",
                format!(
                    "Skipping score {} - {} [{}]: Song was already scrobbled and played again within {}.",
                    artist.bright_blue(),
                    title.bright_blue(),
                    score.beatmap.version.bright_blue(),
                    format!("{} min", self.config.repeat_cooldown_mins).bright_blue(),
                ),
                false,
            );

            return;
        }

        if let Err(error) = validate_scrobble(score, &self.config) {
            Logger::warn(
                "Scrobbler",
                format!(
                    "Skipping score {} - {} [{}]: {error}",
                    artist.bright_blue(),
                    title.bright_blue(),
                    score.beatmap.version.bright_blue(),
                ),
                false,
            );

            return;
        }

        let (new_artist, new_title) = handle_redirects(score, artist, title, &self.config);

        let artist_redirected_text = new_artist.as_ref().map(|new_artist| {
            let old_artist = artist;
            artist = new_artist;
            format!(" (redirected from {})", old_artist.bright_blue())
        });

        let title_redirected_text = new_title.as_ref().map(|new_title| {
            let old_title = title;
            title = new_title;
            format!(" (redirected from {})", old_title.bright_blue())
        });

        self.remember_scrobble(song, played_at);

        let album = if self.config.fetch_album_names { score.get_album_name(artist, title) } else { None };

        Logger::success(
            "Scrobbler",
            format!(
                "New score by {} ({}) found: {}{} - {}{} ({})",
                score.user.username.bright_blue(),
                score.user.id.to_string().bright_blue(),
                artist.bright_blue(),
                artist_redirected_text.as_deref().unwrap_or_default(),
                title.bright_blue(),
                title_redirected_text.as_deref().unwrap_or_default(),
                album.as_deref().unwrap_or("Unknown Album").bright_blue(),
            ),
            false,
        );

        if self.config.log_scrobbles {
            Logger::file(format!("{} | {artist} - {title}", score.ended_at));
        }

        if let Some(last_fm) = self.last_fm.as_ref() {
            match last_fm.scrobble(artist, title, album.as_deref(), score.beatmap.total_length) {
                Ok(_) => Logger::success("Last.fm", "Successfully scrobbled score.", true),
                Err(error) => Logger::error("Last.fm", error, true),
            };
        }

        if let Some(listenbrainz) = self.listenbrainz.as_ref() {
            match listenbrainz.scrobble(artist, title, album.as_deref(), score.beatmap.total_length) {
                Ok(_) => Logger::success("ListenBrainz", "Successfully scrobbled score.", true),
                Err(error) => Logger::error("ListenBrainz", error, true),
            };
        }
    }

    /// Whether this play is a repeat of a song that was scrobbled and kept being played within `repeat_cooldown_mins`.
    /// Every repeated play restarts the cooldown, so retrying a map many times in a row only scrobbles it once.
    fn is_repeat(&mut self, song: &str, played_at: DateTime<FixedOffset>) -> bool {
        let cooldown_secs = self.config.repeat_cooldown_mins as i64 * 60;

        if cooldown_secs == 0 {
            return false;
        }

        let Some(last_played_at) = self.recently_scrobbled.get_mut(song) else { return false };

        if (played_at - *last_played_at).num_seconds() < cooldown_secs {
            *last_played_at = played_at;
            return true;
        }

        false
    }

    fn remember_scrobble(&mut self, song: String, played_at: DateTime<FixedOffset>) {
        let cooldown_secs = self.config.repeat_cooldown_mins as i64 * 60;

        if cooldown_secs == 0 {
            return;
        }

        // Forget songs whose cooldown has already passed
        self.recently_scrobbled.retain(|_, last_played_at| (played_at - *last_played_at).num_seconds() < cooldown_secs);
        self.recently_scrobbled.insert(song, played_at);
    }
}
