//! The palette's catalog names its actions as strings: a typo would silently drop a command.

use super::*;

#[test]
fn every_catalog_name_is_a_registered_action() {
    let known: std::collections::HashSet<&str> = gpui::generate_list_of_all_registered_actions().map(|a| a.name).collect();
    let unknown: Vec<&str> = commands::catalog().into_iter().map(|(action, ..)| action).filter(|name| !known.contains(name)).collect();
    assert!(unknown.is_empty(), "catalog names no action: {unknown:?}");
}
