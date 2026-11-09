//! The two languages of the site and how one is chosen.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Es,
    En,
}

impl Lang {
    pub fn code(self) -> &'static str {
        match self {
            Lang::Es => "es",
            Lang::En => "en",
        }
    }

    pub fn other(self) -> Lang {
        match self {
            Lang::Es => Lang::En,
            Lang::En => Lang::Es,
        }
    }

    fn parse(code: &str) -> Option<Lang> {
        match code.trim().to_ascii_lowercase().split(['-', '_']).next()? {
            "es" => Some(Lang::Es),
            "en" => Some(Lang::En),
            _ => None,
        }
    }
}

/// `?lang=` wins; otherwise the first language the browser asks for that the
/// site has; otherwise Spanish, the language of the app and its community.
pub fn negotiate(query: Option<&str>, accept_language: Option<&str>) -> Lang {
    if let Some(lang) = query.and_then(Lang::parse) {
        return lang;
    }
    accept_language
        .into_iter()
        .flat_map(|header| header.split(','))
        .filter_map(|part| Lang::parse(part.split(';').next().unwrap_or_default()))
        .next()
        .unwrap_or(Lang::Es)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_query_wins_over_the_browser() {
        assert_eq!(negotiate(Some("en"), Some("es-PE,es;q=0.9")), Lang::En);
        assert_eq!(negotiate(Some("ES"), Some("en-US")), Lang::Es);
    }

    #[test]
    fn the_browsers_first_known_language_is_used() {
        assert_eq!(negotiate(None, Some("en-US,en;q=0.9,es;q=0.8")), Lang::En);
        assert_eq!(negotiate(None, Some("fr-FR,es-PE;q=0.8")), Lang::Es);
        assert_eq!(negotiate(None, Some("de,en;q=0.5")), Lang::En);
    }

    #[test]
    fn spanish_is_the_default() {
        assert_eq!(negotiate(None, None), Lang::Es);
        assert_eq!(negotiate(Some("zz"), Some("fr")), Lang::Es);
        assert_eq!(negotiate(Some(""), Some("")), Lang::Es);
    }
}
