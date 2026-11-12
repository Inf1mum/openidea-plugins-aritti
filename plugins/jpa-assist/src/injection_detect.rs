//! Automatic detection of the bean injection type: the style the target class already uses wins over the
//! `injectionStyle` setting unless `injectionAutoDetect` is switched off.
use super::*;
use super::injection_action::{self, Style};
use regex_lite::Regex;

pub(super) fn detect(text: &str) -> Option<Style> {
    let masked = mask(text);
    let annotated = |names: &[&str]| annotations(&masked).iter().any(|annotation| names.contains(&annotation.name));
    if annotated(&["RequiredArgsConstructor", "AllArgsConstructor"]) {
        return Some(Style::Lombok);
    }
    let field_injection = Regex::new(r"@Autowired(?:\([^)]*\))?\s+(?:(?:private|protected|public|final)\s+)*(?:lateinit\s+var\s+\w+\s*:|[\w.<>\[\], ?]+\s+\w+\s*;)").unwrap();
    if field_injection.is_match(&masked) {
        return Some(Style::Field);
    }
    let constructor_injection = Regex::new(r"private\s+final\s+[\w.<>\[\], ?]+\s+\w+\s*;|\(\s*(?:[^)]*,\s*)?private\s+val\s+\w+\s*:").unwrap();
    constructor_injection.is_match(&masked).then_some(Style::Constructor)
}

pub(super) fn style_for(text: &str) -> Result<Style, Error> {
    let configured = injection_action::style()?;
    if !host::setting_bool("injectionAutoDetect").unwrap_or(true) {
        return Ok(configured);
    }
    Ok(detect(text).unwrap_or(configured))
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::mock::MockHost;

    #[test]
    fn existing_injection_styles_are_recognised() {
        assert_eq!(detect("@Service\n@RequiredArgsConstructor\nclass A {\n}\n"), Some(Style::Lombok));
        assert_eq!(detect("@Service\nclass A {\n    @Autowired\n    private OrderRepository orders;\n}\n"), Some(Style::Field));
        assert_eq!(detect("@Service\nclass A {\n    private final OrderRepository orders;\n    A(OrderRepository orders) { this.orders = orders; }\n}\n"), Some(Style::Constructor));
        assert_eq!(detect("class A(private val orders: OrderRepository)\n"), Some(Style::Constructor));
        assert_eq!(detect("@Service\nclass A {\n    @Autowired\n    void set(OrderRepository orders) { }\n}\n"), None);
        assert_eq!(detect("@Service\nclass A {\n}\n"), None);
    }

    #[test]
    fn setting_switches_detection_off_and_the_configured_style_is_the_fallback() {
        let host = MockHost::install();
        host.set_setting("injectionStyle", "\"field-autowired\"");
        let lombok = "@RequiredArgsConstructor\nclass A {\n}\n";
        assert_eq!(style_for(lombok).unwrap(), Style::Lombok);
        assert_eq!(style_for("class A {\n}\n").unwrap(), Style::Field);
        host.set_setting("injectionAutoDetect", "false");
        assert_eq!(style_for(lombok).unwrap(), Style::Field);
    }
}
