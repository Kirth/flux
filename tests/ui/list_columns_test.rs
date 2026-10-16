use flux::model::{SortBy, UIConfig};
use flux::ui::list_columns::{format_list_date, format_permissions, ListColumn};

#[test]
fn test_format_permissions_file_and_dir() {
    assert_eq!(format_permissions(0o100644).as_deref(), Some("-rw-r--r--"));
    assert_eq!(format_permissions(0o100600).as_deref(), Some("-rw-------"));
    assert_eq!(format_permissions(0o040755).as_deref(), Some("drwxr-xr-x"));
    assert_eq!(format_permissions(0o120777).as_deref(), Some("lrwxrwxrwx"));
}

#[test]
fn test_format_permissions_special_bits() {
    // setuid with and without owner exec, setgid, sticky.
    assert_eq!(format_permissions(0o104755).as_deref(), Some("-rwsr-xr-x"));
    assert_eq!(format_permissions(0o104644).as_deref(), Some("-rwSr--r--"));
    assert_eq!(format_permissions(0o102755).as_deref(), Some("-rwxr-sr-x"));
    assert_eq!(format_permissions(0o041777).as_deref(), Some("drwxrwxrwt"));
    assert_eq!(format_permissions(0o041776).as_deref(), Some("drwxrwxrwT"));
}

#[test]
fn test_format_permissions_unknown_mode() {
    assert_eq!(format_permissions(0), None);
}

#[test]
fn test_format_list_date_missing_and_today() {
    assert_eq!(format_list_date(0), None);
    assert_eq!(format_list_date(-5), None);

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let today = format_list_date(now).unwrap();
    assert!(today.contains(':'), "expected a time in {today:?}");

    // Far in the past renders as a full date, not Today/Yesterday.
    let old = format_list_date(86_400 * 365).unwrap();
    assert!(old.contains("1971") || old.contains("71"), "got {old:?}");
}

#[test]
fn test_list_column_keys_round_trip() {
    for col in ListColumn::ALL {
        assert_eq!(ListColumn::from_key(col.key()), Some(col));
    }
    assert_eq!(ListColumn::from_key("bogus"), None);
}

#[test]
fn test_list_column_sort_mapping() {
    assert_eq!(ListColumn::Size.sort_by(), Some(SortBy::Size));
    assert_eq!(ListColumn::Modified.sort_by(), Some(SortBy::Date));
    assert_eq!(ListColumn::Type.sort_by(), Some(SortBy::Type));
    assert_eq!(ListColumn::Owner.sort_by(), None);
}

#[test]
fn test_list_columns_config_defaults_and_unknown_keys() {
    assert_eq!(
        UIConfig::default().list_columns,
        ["size", "modified", "type"]
    );

    let ui: UIConfig = toml::from_str("list_columns = [\"owner\", \"bogus\"]").unwrap();
    let cols: Vec<_> = ui
        .list_columns
        .iter()
        .filter_map(|k| ListColumn::from_key(k))
        .collect();
    assert_eq!(cols, [ListColumn::Owner]);
    assert!(ui.list_column_widths.is_empty());
}

#[test]
fn test_fit_columns_drops_from_the_right() {
    use flux::ui::list_columns::fit_columns;

    let widths = [90, 150, 150, 150, 90, 110];
    let all = [true; 6];
    // 160 name + 10 gap; then columns in order until one doesn't fit.
    assert_eq!(fit_columns(all, widths, i32::MAX), [true; 6]);
    assert_eq!(
        fit_columns(all, widths, 170 + 90 + 150),
        [true, true, false, false, false, false]
    );
    assert_eq!(fit_columns(all, widths, 200), [false; 6]);
    // Hidden columns don't take space; the next visible one can still fit.
    let some = [true, false, false, true, false, false];
    assert_eq!(
        fit_columns(some, widths, 170 + 90 + 150),
        [true, false, false, true, false, false]
    );
}
