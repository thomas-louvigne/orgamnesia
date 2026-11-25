use super::*;

fn profile(name: &str) -> Profile {
    Profile { id: name.into(), name: name.into(), editor_preset: "classic".into(), app: Bindings::new(), editor: Bindings::new() }
}

#[test]
fn new_profiles_get_a_free_name() {
    assert_eq!(free_name("Perso", &[]), "Perso");
    assert_eq!(free_name("Perso", &[profile("Perso"), profile("Perso 2"), profile("Autre")]), "Perso 3");
}

#[test]
fn empty_bindings_are_dropped() {
    let mut b = Bindings::new();
    b.insert("save".into(), vec![String::new(), "ctrl+s".into()]);
    b.insert("none".into(), vec![String::new()]);
    let b = clean_bindings(b);
    assert_eq!(b.len(), 1);
    assert_eq!(b["save"], ["ctrl+s"]);
}
