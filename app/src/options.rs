//! Command-line flags and the environment switches that are read once at start.
//!
//! Flags: `--selftest`, `--pseudo`, `--scheme light|dark`, `--style <Name>`, `--page <0-4>`, `--collapsed`,
//! `--size WxH`, `--shots <dir>` with `--prefix <name>`. Environment: `SKRADLE_REDUCE_MOTION=1`.

use std::path::PathBuf;
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// Load the window, visit every page and exit (status 0 when everything loaded).
    pub selftest: bool,
    /// Pseudo-localisation: lengthened, accented text to find clipping.
    pub pseudo: bool,
    /// Force a colour scheme: `""` (follow the settings), `"light"` or `"dark"`.
    pub scheme: String,
    pub style: Option<String>,
    pub page: i32,
    pub collapsed: bool,
    pub width: i32,
    pub height: i32,
    /// Save a screenshot of each state into this folder, then exit.
    pub shots: Option<PathBuf>,
    pub prefix: String,
    pub reduce_motion: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            selftest: false,
            pseudo: false,
            scheme: String::new(),
            style: None,
            page: 0,
            collapsed: false,
            width: 1180,
            height: 780,
            shots: None,
            prefix: "shot".to_owned(),
            reduce_motion: false,
        }
    }
}

impl Options {
    /// Parses the arguments after the program name. Flags this program does not know (Qt's own, such as
    /// `-platform`) are ignored; a flag that needs a value but has none is an error.
    pub fn parse<I: IntoIterator<Item = String>>(
        args: I,
        reduce_motion_env: bool,
    ) -> Result<Self, String> {
        let mut options = Options {
            reduce_motion: reduce_motion_env,
            ..Options::default()
        };
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            let mut value = |flag: &str| args.next().ok_or_else(|| format!("{flag} needs a value"));
            match arg.as_str() {
                "--selftest" => options.selftest = true,
                "--pseudo" => options.pseudo = true,
                "--collapsed" => options.collapsed = true,
                "--scheme" => {
                    let scheme = value("--scheme")?;
                    if scheme != "light" && scheme != "dark" {
                        return Err(format!("--scheme must be light or dark, not {scheme}"));
                    }
                    options.scheme = scheme;
                }
                "--style" => options.style = Some(value("--style")?),
                "--page" => {
                    let page = value("--page")?;
                    options.page = page
                        .parse::<i32>()
                        .ok()
                        .filter(|p| (0..5).contains(p))
                        .ok_or_else(|| format!("--page must be 0 to 4, not {page}"))?;
                }
                "--size" => {
                    let size = value("--size")?;
                    let (w, h) = size
                        .split_once('x')
                        .and_then(|(w, h)| Some((w.parse::<i32>().ok()?, h.parse::<i32>().ok()?)))
                        .filter(|(w, h)| *w >= 640 && *h >= 480)
                        .ok_or_else(|| format!("--size must look like 1180x780, not {size}"))?;
                    options.width = w;
                    options.height = h;
                }
                "--shots" => options.shots = Some(PathBuf::from(value("--shots")?)),
                "--prefix" => options.prefix = value("--prefix")?,
                _ => {}
            }
        }
        Ok(options)
    }
}

static OPTIONS: OnceLock<Options> = OnceLock::new();
static STYLE: OnceLock<String> = OnceLock::new();

/// Stores the options for the QML singletons (which Qt creates itself, with no way to pass them in).
pub fn set(options: Options) {
    let _ = OPTIONS.set(options);
}

/// The options set at start (defaults if none were set, as in unit tests).
pub fn get() -> &'static Options {
    OPTIONS.get_or_init(Options::default)
}

/// The Qt Quick Controls style in use, for the About page.
pub fn set_style_name(name: String) {
    let _ = STYLE.set(name);
}

pub fn style_name() -> String {
    STYLE.get().cloned().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Options, String> {
        Options::parse(args.iter().map(|a| (*a).to_owned()), false)
    }

    #[test]
    fn defaults() {
        assert_eq!(parse(&[]).unwrap(), Options::default());
    }

    #[test]
    fn reads_every_flag() {
        let o = parse(&[
            "--selftest",
            "--pseudo",
            "--collapsed",
            "--scheme",
            "dark",
            "--style",
            "Basic",
            "--page",
            "3",
            "--size",
            "900x600",
            "--shots",
            "out",
            "--prefix",
            "dark",
        ])
        .unwrap();
        assert!(o.selftest && o.pseudo && o.collapsed);
        assert_eq!(
            (o.scheme.as_str(), o.page, o.width, o.height),
            ("dark", 3, 900, 600)
        );
        assert_eq!(o.style.as_deref(), Some("Basic"));
        assert_eq!(o.shots, Some(PathBuf::from("out")));
        assert_eq!(o.prefix, "dark");
    }

    #[test]
    fn rejects_bad_values() {
        assert!(parse(&["--scheme", "pink"]).is_err());
        assert!(parse(&["--page", "9"]).is_err());
        assert!(parse(&["--size", "10x10"]).is_err());
        assert!(parse(&["--shots"]).is_err());
    }

    #[test]
    fn ignores_flags_that_belong_to_qt() {
        assert_eq!(
            parse(&["-platform", "offscreen"]).unwrap(),
            Options::default()
        );
    }

    #[test]
    fn environment_switch_is_kept() {
        assert!(Options::parse(Vec::new(), true).unwrap().reduce_motion);
    }
}
