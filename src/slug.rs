/// Long enough to be recognizable, short enough for Ark's fixed-width list
/// and picker layouts. This applies to generated aliases only; a user may
/// intentionally choose a longer alias.
pub const MAX_ALIAS_LENGTH: usize = 24;

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

/// Produces a compact, readable alias from provider-derived text.
pub fn auto_slugify(input: &str) -> String {
    shorten_slug(slugify(input))
}

fn shorten_slug(slug: String) -> String {
    if slug.len() <= MAX_ALIAS_LENGTH {
        return slug;
    }

    let prefix = &slug[..MAX_ALIAS_LENGTH];
    // Avoid leaving a clipped word when the useful boundary fits. A single
    // long word still has a deterministic hard limit.
    prefix
        .rfind('-')
        .filter(|&boundary| boundary > 0)
        .map(|boundary| prefix[..boundary].to_owned())
        .unwrap_or_else(|| prefix.to_owned())
}

#[cfg(test)]
mod tests {
    use super::{MAX_ALIAS_LENGTH, auto_slugify, slugify};

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

    #[test]
    fn automatic_slugs_cap_at_a_word_boundary() {
        assert_eq!(
            auto_slugify("is the ark auto feature over engineered"),
            "is-the-ark-auto-feature"
        );
    }

    #[test]
    fn automatic_single_long_word_is_capped() {
        assert_eq!(
            auto_slugify(&"a".repeat(MAX_ALIAS_LENGTH + 1)).len(),
            MAX_ALIAS_LENGTH
        );
    }

    #[test]
    fn user_slug_is_not_shortened() {
        let alias = "this-is-an-intentionally-long-user-defined-alias";
        assert_eq!(slugify(alias), alias);
    }
}
