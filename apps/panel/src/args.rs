//! Command-line options. None of them is persisted: they affect only this
//! run of the panel and never change the user's configuration.

use std::path::PathBuf;
use std::time::Duration;

use windowsrice_ui::legibility::BackplateMode;

#[derive(Debug, Clone, Default)]
pub struct Args {
    /// Exit after this long (tests and measurements).
    pub exit_after: Option<Duration>,
    /// Write diagnostics JSON here on exit.
    pub stats: Option<PathBuf>,
    /// Forced UI scale instead of the monitor's DPI (simulated DPI test).
    pub scale: Option<f32>,
    /// Monitor index from enumeration order; default is the primary monitor.
    pub monitor: Option<usize>,
    /// Theme id to show instead of the configured one (not saved).
    pub theme: Option<String>,
    /// Force dark or light mode instead of following Windows.
    pub mode: Option<windowsrice_theme::Mode>,
    pub opacity: Option<f32>,
    pub backplates: BackplateMode,
    /// Treat transparency as off regardless of the Windows setting.
    pub reduced_transparency: bool,
    /// Do not register the focus hotkey.
    pub no_hotkey: bool,
    /// Simulate a device loss after this long (resource/recovery test).
    pub simulate_device_loss_after: Option<Duration>,
    /// Simulate a device loss every interval (cleanup stress test).
    pub device_loss_every: Option<Duration>,
    /// Hide (as in Gaming Mode) after `.0`, show again after `.1`.
    pub suspend_test: Option<(Duration, Duration)>,
    /// Override the displayed language: `en` or `ko`.
    pub lang: Option<String>,
    /// Do not query GPU performance counters (GPU shown as unavailable).
    pub no_gpu_counter: bool,
    /// Configuration file to use instead of %APPDATA%\windowsRICE\config.toml
    /// (tests use a project-local file). Saved settings go here too.
    pub config: Option<PathBuf>,
    /// Open the Design Studio (in the running instance if there is one).
    pub studio: bool,
    /// Ask a running bar to exit, then exit.
    pub quit: bool,
    /// Fixed active-window title (deterministic screenshots).
    pub title: Option<String>,
}

fn secs(v: Option<String>, flag: &str) -> Result<Duration, String> {
    v.ok_or_else(|| format!("{flag} needs a value"))?
        .parse::<f64>()
        .map(Duration::from_secs_f64)
        .map_err(|e| format!("{flag}: {e}"))
}

impl Args {
    pub fn parse(mut it: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut a = Args::default();
        while let Some(flag) = it.next() {
            match flag.as_str() {
                "--exit-after" => a.exit_after = Some(secs(it.next(), &flag)?),
                "--stats" => a.stats = it.next().map(PathBuf::from),
                "--scale" => {
                    let s: f32 = it
                        .next()
                        .ok_or("--scale needs a value")?
                        .parse()
                        .map_err(|e| format!("--scale: {e}"))?;
                    if !(0.5..=4.0).contains(&s) {
                        return Err("--scale must be between 0.5 and 4".into());
                    }
                    a.scale = Some(s);
                }
                "--monitor" => {
                    a.monitor = Some(
                        it.next()
                            .ok_or("--monitor needs a value")?
                            .parse()
                            .map_err(|e| format!("--monitor: {e}"))?,
                    );
                }
                "--theme" => a.theme = it.next(),
                "--mode" => {
                    a.mode = match it.next().as_deref() {
                        Some("dark") => Some(windowsrice_theme::Mode::Dark),
                        Some("light") => Some(windowsrice_theme::Mode::Light),
                        _ => return Err("--mode must be dark or light".into()),
                    }
                }
                "--opacity" => {
                    a.opacity = Some(
                        it.next()
                            .ok_or("--opacity needs a value")?
                            .parse()
                            .map_err(|e| format!("--opacity: {e}"))?,
                    );
                }
                "--backplates" => {
                    a.backplates = match it.next().as_deref() {
                        Some("auto") => BackplateMode::Auto,
                        Some("always") => BackplateMode::Always,
                        Some("never") => BackplateMode::Never,
                        _ => return Err("--backplates must be auto, always or never".into()),
                    }
                }
                "--reduced-transparency" => a.reduced_transparency = true,
                "--no-hotkey" => a.no_hotkey = true,
                "--no-gpu-counter" => a.no_gpu_counter = true,
                "--studio" => a.studio = true,
                "--quit" => a.quit = true,
                "--simulate-device-loss" => {
                    a.simulate_device_loss_after = Some(secs(it.next(), &flag)?);
                }
                "--device-loss-every" => a.device_loss_every = Some(secs(it.next(), &flag)?),
                "--suspend-test" => {
                    let hide = secs(it.next(), &flag)?;
                    let show = secs(it.next(), &flag)?;
                    a.suspend_test = Some((hide, show));
                }
                "--lang" => a.lang = it.next(),
                "--config" => {
                    a.config = Some(PathBuf::from(it.next().ok_or("--config needs a path")?))
                }
                "--title" => a.title = it.next(),
                "--help" | "-h" => return Err(HELP.into()),
                other => return Err(format!("unknown option {other}\n{HELP}")),
            }
        }
        Ok(a)
    }
}

pub const HELP: &str = "windowsrice-panel [options]
  --exit-after <s>             exit after s seconds
  --stats <file>               write diagnostics JSON on exit
  --scale <factor>             forced UI scale (simulated DPI)
  --monitor <index>            only this monitor (enumeration order)
  --theme <id>                 show a theme without saving it
  --mode dark|light            force a colour mode
  --opacity <0.3-1.0>          surface opacity override
  --backplates auto|always|never
  --reduced-transparency       draw solid surfaces
  --no-hotkey                  do not register the focus hotkey
  --no-gpu-counter             do not read GPU performance counters
  --studio                     open Design Studio (also in a running bar)
  --quit                       close a running windowsRICE bar
  --simulate-device-loss <s>   simulate one device loss
  --device-loss-every <s>      simulate device loss repeatedly
  --suspend-test <hide> <show> hide and show again (Gaming Mode path)
  --lang en|ko                 display language
  --config <file>              use this config.toml (settings are saved there)
  --title <text>               fixed active-window title";

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(v: &[&str]) -> Result<Args, String> {
        Args::parse(v.iter().map(|s| (*s).to_owned()))
    }

    #[test]
    fn parses_options() {
        let a = parse(&[
            "--exit-after",
            "5",
            "--scale",
            "1.25",
            "--backplates",
            "never",
            "--suspend-test",
            "2",
            "4",
        ])
        .unwrap();
        assert_eq!(a.exit_after, Some(Duration::from_secs(5)));
        assert_eq!(a.scale, Some(1.25));
        assert_eq!(a.backplates, BackplateMode::Never);
        assert_eq!(
            a.suspend_test,
            Some((Duration::from_secs(2), Duration::from_secs(4)))
        );
    }

    #[test]
    fn rejects_bad_values() {
        assert!(parse(&["--scale", "9"]).is_err());
        assert!(parse(&["--mode", "blue"]).is_err());
        assert!(parse(&["--bogus"]).is_err());
    }
}
