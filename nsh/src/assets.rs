//! All bundled SVGs, named from their asset-relative paths by build.rs.
#[allow(dead_code)]
pub(crate) mod svg {
    include!(concat!(env!("OUT_DIR"), "/svg.rs"));
}

#[cfg(test)]
mod tests {
    #[test]
    fn generated_assets_are_available_and_parse_as_svg() {
        assert!(!super::svg::ALL.is_empty());
        for (path, source) in super::svg::ALL {
            let document = roxmltree::Document::parse(source)
                .unwrap_or_else(|error| panic!("{path}: {error}"));
            assert_eq!(document.root_element().tag_name().name(), "svg", "{path}");
        }
    }
}
