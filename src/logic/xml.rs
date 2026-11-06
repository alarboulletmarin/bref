//! XML text escaping, the one place for the SVG writers.

/// Text for an element body: `&`, `<` and `>` escaped (what a diagram file's metadata is, and
/// what older versions read back).
pub fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Text for an attribute value or a body: as `escape`, and `"` too.
pub fn attr(text: &str) -> String {
    escape(text).replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    #[test]
    fn escapes_markup_and_only_attributes_escape_quotes() {
        assert_eq!(super::escape("a<b&c>\"d\""), "a&lt;b&amp;c&gt;\"d\"");
        assert_eq!(super::attr("a<b&c>\"d\""), "a&lt;b&amp;c&gt;&quot;d&quot;");
        assert_eq!(super::escape("&amp;"), "&amp;amp;");
    }
}
