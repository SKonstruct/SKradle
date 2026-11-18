//! Files a mod may not replace.
//!
//! The list is the game's own compiled config databases: the 52 `config/<name>.dat` files that sit at the top of
//! `rsrc/config/` in the game's resource bundles (`intro-bundle.jar`, `full-rest-bundle.jar`; checked file by file
//! against a real install, and no other top-level `config/` file exists there). A game update replaces them, so a
//! copy shipped inside a mod goes stale at the next update and then overrides the newer data. A mod that ships one
//! is still applied; only that file is left out, and the apply report says so.
//!
//! Entries are matched against the path inside the mod, case-insensitively (the file systems the launcher runs on
//! fold case). `config/<name>.xml` is deliberately not listed: no game bundle contains such a file, so there is
//! nothing to protect.

const NAMES: [&str; 52] = [
    "accessory",
    "actor",
    "area",
    "attack",
    "battle_sprite",
    "behavior",
    "catalog",
    "conversation",
    "cursor",
    "depot_catalog",
    "depth_scale",
    "description",
    "effect",
    "emote",
    "event",
    "fire_action",
    "font",
    "forge_property",
    "gift",
    "ground",
    "harness",
    "interact",
    "interface_script",
    "item",
    "item_depth_weight",
    "item_property",
    "level_table",
    "material",
    "mission",
    "mission_group",
    "mission_property",
    "parameterized_handler",
    "path",
    "placeable",
    "recipe",
    "recipe_property",
    "render_effect",
    "render_queue",
    "render_scheme",
    "scene_global",
    "shader",
    "sounder",
    "spawn_table",
    "status_condition",
    "style",
    "texture",
    "tile",
    "tile_replacement",
    "uplink",
    "variant",
    "variant_table",
    "wall",
];

/// True when `path` (a validated archive path, `/`-separated) is a protected game file.
pub fn is_protected(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let Some(file) = lower.strip_prefix("config/") else {
        return false;
    };
    file.strip_suffix(".dat")
        .is_some_and(|stem| NAMES.contains(&stem))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protects_the_game_configs_in_any_case() {
        assert!(is_protected("config/item.dat"));
        assert!(is_protected("config/Wall.DAT"));
        assert!(is_protected("CONFIG/variant_table.dat"));
        assert_eq!(NAMES.len(), 52);
    }

    #[test]
    fn leaves_everything_else_alone() {
        for path in [
            "config/item.xml",
            "config/item.dat.bak",
            "config/sub/item.dat",
            "config/unknown.dat",
            "rsrc/config/item.dat",
            "item.dat",
            "texture/ok.png",
            "config/",
        ] {
            assert!(!is_protected(path), "{path}");
        }
    }

    #[test]
    fn the_list_has_no_duplicates() {
        let mut sorted = NAMES.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), NAMES.len());
    }
}
