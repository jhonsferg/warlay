//! The pages of the site, in Spanish and English. Plain HTML, no scripts: they
//! load fast, work everywhere and give search engines the whole text.

use serde_json::json;

use super::i18n::Lang;

pub const GITHUB: &str = "https://github.com/jhonsferg/warlay";
pub const RELEASES: &str = "https://github.com/jhonsferg/warlay/releases/latest";
const ISSUES: &str = "https://github.com/jhonsferg/warlay/issues";
const SECURITY_REPORT: &str = "https://github.com/jhonsferg/warlay/security/advisories/new";
const GOOGLE_POLICY: &str = "https://developers.google.com/terms/api-services-user-data-policy";

/// The text for `lang`, so each sentence sits next to its translation.
fn tr(lang: Lang, es: &'static str, en: &'static str) -> &'static str {
    match lang {
        Lang::Es => es,
        Lang::En => en,
    }
}

/// A site path with the language marker the English version needs.
pub fn href(path: &str, lang: Lang) -> String {
    match lang {
        Lang::Es => path.to_string(),
        Lang::En if path.contains('?') => format!("{path}&lang=en"),
        Lang::En => format!("{path}?lang=en"),
    }
}

pub struct Page<'a> {
    pub lang: Lang,
    /// Path without the language marker, e.g. `/privacy`.
    pub path: &'a str,
    pub title: &'a str,
    pub description: &'a str,
    pub body: String,
    pub json_ld: Option<String>,
}

/// Escapes text for HTML (the pages only interpolate fixed strings, but the
/// helper keeps that true when someone adds a dynamic one).
pub fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub fn render(base: &str, page: &Page) -> String {
    let lang = page.lang;
    let canonical = format!("{base}{}", href(page.path, lang));
    let es_url = format!("{base}{}", href(page.path, Lang::Es));
    let en_url = format!("{base}{}", href(page.path, Lang::En));
    let title = escape(page.title);
    let description = escape(page.description);
    let locale = match lang {
        Lang::Es => "es_PE",
        Lang::En => "en_US",
    };
    // The in-page links keep the language marker before the `#`.
    let anchor_features = format!("{}#features", href("/", lang));
    let anchor_faq = format!("{}#faq", href("/", lang));
    let json_ld = page
        .json_ld
        .as_deref()
        .map(|json| format!("<script type=\"application/ld+json\">{json}</script>"))
        .unwrap_or_default();
    format!(
        r##"<!doctype html>
<html lang="{lang}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<meta name="description" content="{description}">
<meta name="theme-color" content="#14110b">
<meta name="robots" content="index, follow">
<link rel="canonical" href="{canonical}">
<link rel="alternate" hreflang="es" href="{es_url}">
<link rel="alternate" hreflang="en" href="{en_url}">
<link rel="alternate" hreflang="x-default" href="{es_url}">
<link rel="icon" href="/favicon.svg" type="image/svg+xml">
<link rel="stylesheet" href="/assets/style.css">
<meta property="og:type" content="website">
<meta property="og:site_name" content="Warlay">
<meta property="og:title" content="{title}">
<meta property="og:description" content="{description}">
<meta property="og:url" content="{canonical}">
<meta property="og:locale" content="{locale}">
<meta property="og:image" content="{base}/assets/social-preview.png">
<meta property="og:image:width" content="1280">
<meta property="og:image:height" content="640">
<meta name="twitter:card" content="summary_large_image">
<meta name="twitter:title" content="{title}">
<meta name="twitter:description" content="{description}">
<meta name="twitter:image" content="{base}/assets/social-preview.png">
{json_ld}
</head>
<body>
<a class="skip" href="#main">{skip}</a>
<header class="site">
<div class="wrap">
<a class="brand" href="{home}"><img src="/assets/logo.svg" alt="" width="36" height="36"><span>WARLAY</span></a>
<nav class="site" aria-label="{nav_label}">
<a href="{features}">{features_label}</a>
<a href="{faq}">FAQ</a>
<a href="{github}">GitHub</a>
<a class="lang" href="{switch}" hreflang="{other}" lang="{other}">{other_name}</a>
</nav>
</div>
</header>
<main id="main">
{body}
</main>
<footer class="site">
<div class="wrap">
<a href="{privacy}">{privacy_label}</a>
<a href="{terms}">{terms_label}</a>
<a href="{github}">GitHub</a>
<a href="{issues}">{support}</a>
<p class="legal">{legal}</p>
</div>
</footer>
</body>
</html>
"##,
        body = page.body,
        lang = lang.code(),
        skip = tr(lang, "Saltar al contenido", "Skip to content"),
        home = href("/", lang),
        nav_label = tr(lang, "Principal", "Main"),
        features = anchor_features,
        features_label = tr(lang, "Características", "Features"),
        faq = anchor_faq,
        github = GITHUB,
        switch = href(page.path, lang.other()),
        other = lang.other().code(),
        other_name = match lang.other() {
            Lang::Es => "Español",
            Lang::En => "English",
        },
        privacy = href("/privacy", lang),
        privacy_label = tr(lang, "Privacidad", "Privacy"),
        terms = href("/terms", lang),
        terms_label = tr(lang, "Términos", "Terms"),
        issues = ISSUES,
        support = tr(lang, "Soporte", "Support"),
        legal = tr(
            lang,
            "Warlay es software libre bajo licencia MIT. Warcraft III es una marca de Blizzard Entertainment; este proyecto no está afiliado ni respaldado por Blizzard.",
            "Warlay is free software under the MIT license. Warcraft III is a trademark of Blizzard Entertainment; this project is not affiliated with or endorsed by Blizzard.",
        ),
    )
}

pub fn landing(base: &str, lang: Lang) -> String {
    let faq: [(&str, &str); 5] = [
        (
            tr(lang, "¿Cómo juego Warcraft III en LAN por internet?", "How do I play Warcraft III LAN games over the internet?"),
            tr(
                lang,
                "Instala el cliente de Warlay, inicia sesión y crea o únete a una sala. El anfitrión crea la partida en Red de área local y aparece para todos los de la sala.",
                "Install the Warlay client, sign in, and create or join a room. The host creates the game in Local Area Network and it shows up for everyone in the room.",
            ),
        ),
        (
            tr(lang, "¿Necesito una VPN o abrir puertos en mi router?", "Do I need a VPN or to open ports on my router?"),
            tr(
                lang,
                "No. Los jugadores se conectan directamente entre sí mediante perforación de NAT y, si no es posible, a través de un relé del servidor.",
                "No. Players connect directly to each other with NAT hole punching and, when that is not possible, through a server relay.",
            ),
        ),
        (
            tr(lang, "¿Es una alternativa a Hamachi, Radmin VPN o ZeroTier?", "Is it an alternative to Hamachi, Radmin VPN or ZeroTier?"),
            tr(
                lang,
                "Para jugar Warcraft III en LAN, sí: está hecho para eso y no necesita adaptadores de red virtuales ni configuración del router.",
                "For Warcraft III LAN play, yes: it is built for that and needs no virtual network adapter or router configuration.",
            ),
        ),
        (
            tr(lang, "¿Por qué pide permisos de administrador?", "Why does it ask for administrator permission?"),
            tr(
                lang,
                "Para capturar el anuncio de partida LAN del anfitrión, Windows exige acceso a sockets en bruto. El cliente no expone ningún servicio local a otros programas.",
                "To capture the host's LAN game announcement, Windows requires raw socket access. The client exposes no local service to other programs.",
            ),
        ),
        (
            tr(lang, "¿Es gratis?", "Is it free?"),
            tr(
                lang,
                "Sí. Warlay es de código abierto bajo licencia MIT.",
                "Yes. Warlay is open source under the MIT license.",
            ),
        ),
    ];
    let faq_html: String = faq
        .iter()
        .map(|(q, a)| {
            format!(
                "<details><summary>{}</summary><p>{}</p></details>\n",
                escape(q),
                escape(a)
            )
        })
        .collect();
    let json_ld = json!([
        {
            "@context": "https://schema.org",
            "@type": "SoftwareApplication",
            "name": "Warlay",
            "applicationCategory": "GameApplication",
            "operatingSystem": "Windows 10, Windows 11",
            "description": tr(lang, "Juega partidas LAN de Warcraft III por internet con tus amigos, sin VPN ni reenvío de puertos.", "Play Warcraft III LAN games over the internet with friends, no VPN and no port forwarding."),
            "url": base,
            "downloadUrl": RELEASES,
            "license": "https://opensource.org/licenses/MIT",
            "isAccessibleForFree": true,
            "offers": { "@type": "Offer", "price": "0", "priceCurrency": "USD" },
            "inLanguage": ["es", "en"]
        },
        {
            "@context": "https://schema.org",
            "@type": "FAQPage",
            "mainEntity": faq.iter().map(|(q, a)| json!({
                "@type": "Question",
                "name": q,
                "acceptedAnswer": { "@type": "Answer", "text": a }
            })).collect::<Vec<_>>()
        }
    ])
    .to_string();

    let feature = |title: &str, text: &str| format!("<li><h3>{}</h3><p>{}</p></li>\n", escape(title), escape(text));
    let features = [
        feature(
            tr(lang, "Sin VPN ni puertos", "No VPN, no port forwarding"),
            tr(lang, "Inicia sesión y entra a una sala: la partida del anfitrión aparece sola en tu lista de Red de área local.", "Sign in and join a room: the host's game appears in your Local Area Network list by itself."),
        ),
        feature(
            tr(lang, "Baja latencia", "Low latency"),
            tr(lang, "Conexión directa entre jugadores sobre QUIC, con relé automático solo cuando hace falta.", "Direct player-to-player connections over QUIC, with an automatic relay only when needed."),
        ),
        feature(
            tr(lang, "Amigos y comunidades", "Friends and communities"),
            tr(lang, "Agrega amigos, mira quién está conectado, únete a comunidades y recibe invitaciones en tiempo real.", "Add friends, see who is online, join communities and get invitations in real time."),
        ),
        feature(
            tr(lang, "Chat y voz", "Chat and voice"),
            tr(lang, "Chat de sala y mensajes privados cifrados de extremo a extremo, más chat de voz con modo de baja latencia.", "Room chat and end-to-end encrypted private messages, plus voice chat with a low-latency mode."),
        ),
        feature(
            tr(lang, "Cuenta propia o Google", "Your own account or Google"),
            tr(lang, "Entra con usuario y contraseña o con tu cuenta de Google, y mantén los mismos datos con cualquiera de los dos.", "Sign in with a username and password or with your Google account, and keep the same data with either."),
        ),
        feature(
            tr(lang, "Código abierto", "Open source"),
            tr(lang, "El cliente y el servidor son software libre (MIT). Puedes revisarlo, mejorarlo o alojar tu propio servidor.", "The client and the server are free software (MIT). Review it, improve it, or host your own server."),
        ),
    ]
    .concat();

    let body = format!(
        r##"<div class="wrap">
<section class="hero">
<div>
<h1>{h1}</h1>
<p class="lead">{lead}</p>
<div class="cta">
<a class="btn btn-primary" href="{releases}">{download}</a>
<a class="btn" href="{github}">{source}</a>
</div>
<p class="note">{note}</p>
</div>
<img class="mark" src="/assets/logo.svg" alt="{logo_alt}" width="260" height="260">
</section>
<section class="block" id="features">
<h2>{features_title}</h2>
<ul class="cards">
{features}</ul>
</section>
<section class="block" id="how">
<h2>{how_title}</h2>
<ol class="steps">
<li><div><strong>{s1}</strong><span>{s1d}</span></div></li>
<li><div><strong>{s2}</strong><span>{s2d}</span></div></li>
<li><div><strong>{s3}</strong><span>{s3d}</span></div></li>
</ol>
</section>
<section class="block" id="faq">
<h2>FAQ</h2>
{faq_html}</section>
</div>"##,
        h1 = tr(lang, "Juega Warcraft III en LAN por internet", "Play Warcraft III LAN games over the internet"),
        lead = tr(
            lang,
            "Warlay conecta a tus amigos y a tu comunidad para jugar partidas LAN de Warcraft III en línea. Sin VPN, sin reenvío de puertos y gratis.",
            "Warlay connects your friends and community to play Warcraft III LAN games online. No VPN, no port forwarding, and free.",
        ),
        releases = RELEASES,
        download = tr(lang, "Descargar para Windows", "Download for Windows"),
        github = GITHUB,
        source = tr(lang, "Ver el código", "View the source"),
        note = tr(lang, "Windows 10 y 11 (x64) · Licencia MIT", "Windows 10 and 11 (x64) · MIT license"),
        logo_alt = tr(lang, "Logotipo de Warlay", "Warlay logo"),
        features_title = tr(lang, "Todo lo que necesitas para jugar con tu gente", "Everything you need to play with your people"),
        how_title = tr(lang, "Cómo funciona", "How it works"),
        s1 = tr(lang, "Instala y entra", "Install and sign in"),
        s1d = tr(lang, "Descarga el instalador, abre Warlay e inicia sesión con tu cuenta o con Google.", "Download the installer, open Warlay and sign in with your account or Google."),
        s2 = tr(lang, "Crea o únete a una sala", "Create or join a room"),
        s2d = tr(lang, "Elige una sala de tus amigos o de una comunidad, o crea la tuya.", "Pick a room from your friends or a community, or create your own."),
        s3 = tr(lang, "Juega", "Play"),
        s3d = tr(lang, "El anfitrión crea la partida en Red de área local y aparece para todos. Warlay lanza Warcraft III por ti.", "The host creates the game in Local Area Network and it appears for everyone. Warlay launches Warcraft III for you."),
    );
    render(
        base,
        &Page {
            lang,
            path: "/",
            title: tr(
                lang,
                "Warlay - Juega Warcraft III en LAN por internet, sin VPN",
                "Warlay - Play Warcraft III LAN games over the internet, no VPN",
            ),
            description: tr(
                lang,
                "Warlay te permite jugar partidas LAN de Warcraft III por internet con amigos y comunidades: sin VPN ni reenvío de puertos, conexión P2P de baja latencia, chat y voz. Gratis y de código abierto.",
                "Warlay lets you play Warcraft III LAN games over the internet with friends and communities: no VPN or port forwarding, low-latency P2P, chat and voice. Free and open source.",
            ),
            body,
            json_ld: Some(json_ld),
        },
    )
}

/// A legal page: a title, the date, and sections of paragraphs.
fn document(
    base: &str,
    lang: Lang,
    path: &str,
    title: &str,
    description: &str,
    sections: &[(String, Vec<String>)],
) -> String {
    let mut body = format!(
        "<div class=\"wrap\"><article class=\"doc\">\n<h1>{}</h1>\n<p class=\"updated\">{}</p>\n",
        escape(title),
        tr(
            lang,
            "Última actualización: 9 de noviembre de 2026",
            "Last updated: November 9, 2026"
        )
    );
    for (heading, paragraphs) in sections {
        body.push_str(&format!("<h2>{}</h2>\n", escape(heading)));
        for paragraph in paragraphs {
            // Paragraphs are trusted static text and may carry links.
            if let Some(items) = paragraph.strip_prefix("* ") {
                body.push_str("<ul>\n");
                for item in items.split("\n* ") {
                    body.push_str(&format!("<li>{item}</li>\n"));
                }
                body.push_str("</ul>\n");
            } else {
                body.push_str(&format!("<p>{paragraph}</p>\n"));
            }
        }
    }
    body.push_str("</article></div>");
    render(
        base,
        &Page {
            lang,
            path,
            title,
            description,
            body,
            json_ld: None,
        },
    )
}

fn section(heading: &str, paragraphs: &[&str]) -> (String, Vec<String>) {
    (heading.to_string(), paragraphs.iter().map(|p| p.to_string()).collect())
}

pub fn privacy(base: &str, lang: Lang) -> String {
    let contact = format!(
        "{} <a href=\"{ISSUES}\">GitHub Issues</a> {} <a href=\"{SECURITY_REPORT}\">{}</a>.",
        tr(
            lang,
            "Para cualquier consulta sobre tus datos escribe en",
            "For any question about your data, use"
        ),
        tr(
            lang,
            "o, si es un asunto de seguridad, usa el",
            "or, for a security matter, the"
        ),
        tr(lang, "aviso de seguridad privado", "private security advisory"),
    );
    let google_policy = format!(
        "{} <a href=\"{GOOGLE_POLICY}\">{}</a>, {}",
        tr(
            lang,
            "El uso que Warlay hace de la información recibida de las API de Google se ajusta a la",
            "Warlay's use of information received from Google APIs adheres to the",
        ),
        tr(
            lang,
            "Política de datos de usuario de los servicios de API de Google",
            "Google API Services User Data Policy",
        ),
        tr(
            lang,
            "incluidos los requisitos de uso limitado.",
            "including the Limited Use requirements."
        ),
    );
    let sections = vec![
        section(
            tr(lang, "Quién es el responsable", "Who is responsible"),
            &[tr(
                lang,
                "Warlay es un proyecto de código abierto operado por su mantenedor. Este sitio y el servicio de Warlay se alojan en un servidor propio.",
                "Warlay is an open-source project run by its maintainer. This site and the Warlay service are hosted on a server operated by the project.",
            )],
        ),
        section(
            tr(lang, "Qué datos tratamos", "What data we handle"),
            &[
                tr(lang, "* <strong>Cuenta:</strong> nombre de usuario, correo, nombre visible, color de avatar y biografía. La contraseña se guarda solo como un hash Argon2id, nunca en claro.\n* <strong>Inicio de sesión con Google:</strong> si eliges entrar con Google recibimos tu identificador de Google, tu correo y tu nombre. No accedemos a tus contactos, archivos ni otros datos de Google (ámbitos: openid, email y profile).\n* <strong>Actividad en Warlay:</strong> amigos, comunidades, salas, mensajes, notificaciones y tu última conexión. Los mensajes privados están cifrados de extremo a extremo: el servidor no puede leerlos. Los mensajes de sala y de comunidad se guardan en el servidor.\n* <strong>Datos técnicos:</strong> el agente de usuario de tus sesiones, y la dirección IP en los registros del servidor web (junto con el país aproximado y la ruta solicitada), que se conservan unos 14 días. Los contadores de límite de uso por IP viven en memoria como máximo una hora.\n* <strong>Voz:</strong> el audio viaja directamente entre jugadores o a través de un relé; no se graba ni se almacena.",
                    "* <strong>Account:</strong> username, e-mail, display name, avatar color and bio. The password is stored only as an Argon2id hash, never in clear.\n* <strong>Sign-in with Google:</strong> if you choose Google we receive your Google identifier, your e-mail and your name. We do not access your contacts, files or any other Google data (scopes: openid, email and profile).\n* <strong>Activity in Warlay:</strong> friends, communities, rooms, messages, notifications and your last seen time. Private messages are end-to-end encrypted: the server cannot read them. Room and community messages are stored on the server.\n* <strong>Technical data:</strong> the user agent of your sessions, and the IP address in web server logs (with the approximate country and the requested path), kept for about 14 days. Per-IP rate-limit counters live in memory for at most an hour.\n* <strong>Voice:</strong> audio travels directly between players or through a relay; it is not recorded or stored."),
            ],
        ),
        section(
            tr(lang, "Para qué los usamos", "What we use it for"),
            &[tr(
                lang,
                "Para prestar el servicio (cuentas, salas, chat, voz), mantenerlo seguro y evitar abusos. No vendemos tus datos, no mostramos publicidad y no usamos analítica ni cookies de seguimiento de terceros.",
                "To provide the service (accounts, rooms, chat, voice), keep it secure and prevent abuse. We do not sell your data, show ads, or use third-party analytics or tracking cookies.",
            )],
        ),
        section(
            tr(lang, "Con quién se comparten", "Who it is shared with"),
            &[
                tr(
                    lang,
                    "No compartimos tus datos con terceros, salvo los servicios que tú usas al interactuar con ellos: Google (si eliges iniciar sesión con Google) y GitHub (de donde el cliente descarga las versiones nuevas).",
                    "We do not share your data with third parties, except for the services you use when you interact with them: Google (if you choose to sign in with Google) and GitHub (where the client downloads new versions from).",
                ),
                &google_policy,
            ],
        ),
        section(
            tr(lang, "Conservación y eliminación", "Retention and deletion"),
            &[
                tr(
                    lang,
                    "Conservamos tus datos mientras tengas una cuenta. Puedes pedir que eliminemos tu cuenta y sus datos; por ahora esa solicitud se hace por el contacto indicado abajo. Un respaldo de la base de datos puede tardar un tiempo en desaparecer.",
                    "We keep your data while you have an account. You can ask us to delete your account and its data; for now that request is made through the contact below. A database backup can take a while to disappear.",
                ),
            ],
        ),
        section(
            tr(lang, "Seguridad", "Security"),
            &[tr(
                lang,
                "Usamos TLS en todas las conexiones, contraseñas con Argon2id, sesiones de corta duración y cifrado de extremo a extremo en los mensajes privados. Ningún sistema es infalible: si encuentras un fallo, avísanos por el canal privado.",
                "We use TLS on every connection, Argon2id password hashing, short-lived sessions and end-to-end encryption for private messages. No system is infallible: if you find a flaw, tell us through the private channel.",
            )],
        ),
        section(
            tr(lang, "Menores de edad", "Children"),
            &[tr(
                lang,
                "Warlay no está dirigido a menores de 13 años y no recopilamos a sabiendas datos de ellos.",
                "Warlay is not directed at children under 13 and we do not knowingly collect their data.",
            )],
        ),
        section(
            tr(lang, "Cambios y contacto", "Changes and contact"),
            &[
                tr(
                    lang,
                    "Si esta política cambia, actualizaremos la fecha de esta página.",
                    "If this policy changes, we will update the date on this page.",
                ),
                &contact,
            ],
        ),
    ];
    document(
        base,
        lang,
        "/privacy",
        tr(lang, "Política de privacidad de Warlay", "Warlay Privacy Policy"),
        tr(
            lang,
            "Qué datos trata Warlay, para qué los usa, con quién se comparten y cómo pedir su eliminación.",
            "What data Warlay handles, what it is used for, who it is shared with and how to ask for deletion.",
        ),
        &sections,
    )
}

pub fn terms(base: &str, lang: Lang) -> String {
    let contact = format!(
        "{} <a href=\"{ISSUES}\">GitHub Issues</a>.",
        tr(
            lang,
            "Para consultas sobre estos términos escribe en",
            "For questions about these terms, use"
        ),
    );
    let sections = vec![
        section(
            tr(lang, "Aceptación", "Acceptance"),
            &[tr(
                lang,
                "Al crear una cuenta o usar Warlay aceptas estos términos. Si no estás de acuerdo, no uses el servicio.",
                "By creating an account or using Warlay you accept these terms. If you do not agree, do not use the service.",
            )],
        ),
        section(
            tr(lang, "El servicio", "The service"),
            &[tr(
                lang,
                "Warlay es un servicio gratuito, ofrecido tal cual y sin garantías, para jugar Warcraft III en LAN por internet. Necesitas tu propia copia legítima del juego. Puede cambiar, interrumpirse o terminar en cualquier momento.",
                "Warlay is a free service, provided as is and without warranties, to play Warcraft III LAN games over the internet. You need your own legitimate copy of the game. It may change, be interrupted or end at any time.",
            )],
        ),
        section(
            tr(lang, "Tu cuenta y tu conducta", "Your account and conduct"),
            &[
                tr(lang, "Eres responsable de tu cuenta y de lo que hagas con ella. No debes:\n* acosar, insultar o amenazar a otras personas;\n* compartir contenido ilegal o que infrinja derechos de terceros;\n* hacer trampas, explotar fallos o intentar vulnerar el servicio;\n* suplantar a otra persona ni crear cuentas para evadir una suspensión.",
                    "You are responsible for your account and what you do with it. You must not:\n* harass, insult or threaten other people;\n* share illegal content or content that infringes third-party rights;\n* cheat, exploit bugs or try to attack the service;\n* impersonate another person or create accounts to evade a suspension."),
            ],
        ),
        section(
            tr(lang, "Tu contenido", "Your content"),
            &[tr(
                lang,
                "Conservas los derechos sobre lo que escribes. Nos autorizas a almacenarlo y mostrarlo a quienes lo envíes para que el servicio funcione.",
                "You keep the rights to what you write. You allow us to store it and show it to the people you send it to so that the service works.",
            )],
        ),
        section(
            tr(lang, "Código abierto y marcas", "Open source and trademarks"),
            &[tr(
                lang,
                "El código de Warlay se distribuye bajo licencia MIT. Warcraft III y Blizzard Entertainment son marcas de sus propietarios; Warlay no está afiliado ni respaldado por Blizzard.",
                "Warlay's code is distributed under the MIT license. Warcraft III and Blizzard Entertainment are trademarks of their owners; Warlay is not affiliated with or endorsed by Blizzard.",
            )],
        ),
        section(
            tr(lang, "Suspensión y terminación", "Suspension and termination"),
            &[tr(
                lang,
                "Podemos suspender o eliminar cuentas que incumplan estos términos. Tú puedes dejar de usar Warlay y pedir la eliminación de tu cuenta cuando quieras.",
                "We may suspend or remove accounts that break these terms. You can stop using Warlay and ask for your account to be deleted at any time.",
            )],
        ),
        section(
            tr(lang, "Límite de responsabilidad", "Limitation of liability"),
            &[tr(
                lang,
                "En la medida que la ley lo permita, el proyecto y su mantenedor no responden por pérdidas ni daños derivados del uso del servicio, incluida la pérdida de datos o la interrupción del servicio.",
                "To the extent the law allows, the project and its maintainer are not liable for losses or damages arising from use of the service, including data loss or service interruption.",
            )],
        ),
        section(tr(lang, "Contacto", "Contact"), &[&contact]),
    ];
    document(
        base,
        lang,
        "/terms",
        tr(lang, "Términos del servicio de Warlay", "Warlay Terms of Service"),
        tr(
            lang,
            "Las condiciones para usar Warlay: cuenta, conducta, contenido, marcas y responsabilidad.",
            "The conditions for using Warlay: account, conduct, content, trademarks and liability.",
        ),
        &sections,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "https://warlay.example.com";

    #[test]
    fn the_landing_page_is_complete_in_both_languages() {
        for lang in [Lang::Es, Lang::En] {
            let html = landing(BASE, lang);
            assert!(html.contains(&format!("<html lang=\"{}\">", lang.code())));
            assert!(html.contains("<h1>"));
            assert!(
                html.contains(RELEASES),
                "the download button points at the latest release"
            );
            assert!(html.contains("application/ld+json"));
            assert!(html.contains("FAQPage"));
            assert!(html.contains(&format!("{BASE}/assets/social-preview.png")));
        }
        assert!(landing(BASE, Lang::Es).contains("Juega Warcraft III en LAN por internet"));
        assert!(landing(BASE, Lang::En).contains("Play Warcraft III LAN games over the internet"));
    }

    #[test]
    fn each_language_points_at_the_other_and_declares_a_canonical_url() {
        let es = privacy(BASE, Lang::Es);
        assert!(es.contains(&format!("<link rel=\"canonical\" href=\"{BASE}/privacy\">")));
        assert!(es.contains(&format!("hreflang=\"en\" href=\"{BASE}/privacy?lang=en\"")));
        let en = privacy(BASE, Lang::En);
        assert!(en.contains(&format!("<link rel=\"canonical\" href=\"{BASE}/privacy?lang=en\">")));
        assert!(en.contains(&format!("hreflang=\"x-default\" href=\"{BASE}/privacy\"")));
    }

    #[test]
    fn the_legal_pages_carry_what_google_asks_of_a_privacy_policy() {
        let privacy = privacy(BASE, Lang::En);
        assert!(privacy.contains("Limited Use"));
        assert!(privacy.contains("Argon2id"));
        assert!(privacy.contains("deletion"));
        assert!(terms(BASE, Lang::Es).contains("Blizzard"));
    }

    #[test]
    fn text_is_escaped_and_language_links_keep_the_marker() {
        assert_eq!(escape("<a href=\"x\">&"), "&lt;a href=&quot;x&quot;&gt;&amp;");
        assert_eq!(href("/privacy", Lang::En), "/privacy?lang=en");
        assert_eq!(href("/auth/login?x=1", Lang::En), "/auth/login?x=1&lang=en");
        assert_eq!(href("/privacy", Lang::Es), "/privacy");
    }
}
