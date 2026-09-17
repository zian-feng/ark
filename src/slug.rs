pub fn slugify(input: &str) -> String {
    let mut slug = String::new();
    let mut needs_separator = false;

    for char in input.chars() {
        if char.is_ascii_alphanumeric() {
            if needs_separator && !slug.is_empty() {
                slug.push('-');
            }

            slug.push(char.to_ascii_lowercase());
            needs_separator = false;
        } else if !slug.is_empty() {
            needs_separator = true;
        }
    }

    slug
}

#[cfg(test)]
mod tests {
    use super::slugify;

    #[test]
    fn converts_words_to_kebab_case() {
        assert_eq!(slugify("test name"), "test-name");
    }

    #[test]
    fn lowercases_words() {
        assert_eq!(slugify("Fix OAuth Flow"), "fix-oauth-flow");
    }

    #[test]
    fn collapses_spaces_and_punctuation() {
        assert_eq!(slugify("fix!!!  auth---flow"), "fix-auth-flow");
    }

    #[test]
    fn trims_separators() {
        assert_eq!(slugify("  auth refactor  "), "auth-refactor");
    }

    #[test]
    fn returns_empty_for_no_alphanumeric_text() {
        assert_eq!(slugify("!!!"), "");
    }
}
