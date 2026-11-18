//! The graphics agent's JVM arguments, built from the Graphics page's settings.
//!
//! The agent (`skgfx-agent.jar`, from the `skgfx` repository) is a Java agent: `-javaagent:<jar>` plus
//! `-Dskgfx.*` system properties (see that repository's README for what each one does). The page's anti-aliasing
//! choice has no agent property (supersampling is the render scale), so it is not translated here.
//!
//! The arguments are passed to the game's direct launch as separate arguments, never through a shell, so a jar
//! path with spaces needs no quoting.

use std::path::Path;

/// The Graphics page's values, in the page's own units.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphicsSettings {
    /// Render scale in percent (the agent takes 0.25 to 4.0).
    pub render_scale_percent: u32,
    /// Sharpening strength in percent (the agent takes 0 to 1).
    pub sharpening_percent: u32,
    /// Draw the HUD in its own full-resolution layer.
    pub sharp_hud: bool,
    /// Show the agent's FPS counter (it is on by default in the agent, so only "off" is sent).
    pub show_fps: bool,
    /// Frame generation (experimental, adaptive mode).
    pub frame_generation: bool,
    /// The frame rate frame generation aims for; 0 means the screen's refresh rate.
    pub target_fps: u32,
}

/// A percentage as the agent's decimal: 150 -> `1.5`, 100 -> `1`.
fn fraction(percent: u32) -> String {
    let whole = percent / 100;
    let rest = percent % 100;
    match (rest, rest % 10) {
        (0, _) => whole.to_string(),
        (_, 0) => format!("{whole}.{}", rest / 10),
        _ => format!("{whole}.{rest:02}"),
    }
}

/// `-javaagent:<jar>` followed by the properties. Values are clamped to the agent's ranges, so a hand-edited
/// settings file cannot make the agent refuse to start.
pub fn agent_args(agent_jar: &Path, settings: &GraphicsSettings) -> Vec<String> {
    let mut args = vec![format!("-javaagent:{}", agent_jar.display())];
    args.push(format!(
        "-Dskgfx.scale={}",
        fraction(settings.render_scale_percent.clamp(25, 400))
    ));
    args.push(format!(
        "-Dskgfx.sharpen={}",
        fraction(settings.sharpening_percent.min(100))
    ));
    if settings.sharp_hud {
        args.push("-Dskgfx.hudsplit=true".to_owned());
    }
    if !settings.show_fps {
        args.push("-Dskgfx.counter=false".to_owned());
    }
    if settings.frame_generation {
        args.push("-Dskgfx.fg=true".to_owned());
        args.push(format!(
            "-Dskgfx.fg.target={}",
            settings.target_fps.min(1000)
        ));
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn plain() -> GraphicsSettings {
        GraphicsSettings {
            render_scale_percent: 100,
            sharpening_percent: 0,
            sharp_hud: false,
            show_fps: true,
            frame_generation: false,
            target_fps: 120,
        }
    }

    #[test]
    fn percentages_become_the_agents_decimals() {
        for (percent, text) in [
            (0, "0"),
            (5, "0.05"),
            (40, "0.4"),
            (100, "1"),
            (125, "1.25"),
            (150, "1.5"),
            (200, "2"),
        ] {
            assert_eq!(fraction(percent), text, "{percent}");
        }
    }

    #[test]
    fn the_basic_arguments() {
        let args = agent_args(&PathBuf::from("/opt/skgfx/skgfx-agent.jar"), &plain());
        assert_eq!(
            args,
            [
                "-javaagent:/opt/skgfx/skgfx-agent.jar",
                "-Dskgfx.scale=1",
                "-Dskgfx.sharpen=0"
            ]
        );
    }

    #[test]
    fn switches_add_their_properties_and_a_path_with_spaces_stays_one_argument() {
        let settings = GraphicsSettings {
            render_scale_percent: 150,
            sharpening_percent: 40,
            sharp_hud: true,
            show_fps: false,
            frame_generation: true,
            target_fps: 144,
        };
        let args = agent_args(&PathBuf::from("/Users/a b/skgfx agent.jar"), &settings);
        assert_eq!(args[0], "-javaagent:/Users/a b/skgfx agent.jar");
        for wanted in [
            "-Dskgfx.scale=1.5",
            "-Dskgfx.sharpen=0.4",
            "-Dskgfx.hudsplit=true",
            "-Dskgfx.counter=false",
            "-Dskgfx.fg=true",
            "-Dskgfx.fg.target=144",
        ] {
            assert!(args.contains(&wanted.to_owned()), "{wanted} in {args:?}");
        }
    }

    #[test]
    fn out_of_range_values_are_clamped() {
        let settings = GraphicsSettings {
            render_scale_percent: 5000,
            sharpening_percent: 900,
            sharp_hud: false,
            show_fps: true,
            frame_generation: true,
            target_fps: 99_999,
        };
        let args = agent_args(Path::new("a.jar"), &settings);
        assert!(args.contains(&"-Dskgfx.scale=4".to_owned()));
        assert!(args.contains(&"-Dskgfx.sharpen=1".to_owned()));
        assert!(args.contains(&"-Dskgfx.fg.target=1000".to_owned()));
        let tiny = GraphicsSettings {
            render_scale_percent: 1,
            ..plain()
        };
        assert!(agent_args(Path::new("a.jar"), &tiny).contains(&"-Dskgfx.scale=0.25".to_owned()));
    }
}
