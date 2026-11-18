use std::{
    fmt::Write,
    path::{Component, Path, PathBuf},
    sync::Arc,
};

use axum::{
    extract::{Request, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    Router,
};
use clap::Parser;
use percent_encoding::{percent_decode_str, utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use tower::ServiceExt;
use tower_http::services::ServeFile;

/// Serve a directory over HTTP with image/video/audio preview.
#[derive(Parser)]
struct Args {
    /// Directory to serve.
    #[arg(default_value = ".")]
    dir: PathBuf,
    /// Address to bind.
    #[arg(short, long, default_value = "127.0.0.1")]
    bind: String,
    /// Port to listen on.
    #[arg(short, long, default_value_t = 8080)]
    port: u16,
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Image,
    Video,
    Audio,
    Other,
}

fn kind_of(p: &Path) -> Kind {
    match mime_guess::from_path(p).first() {
        Some(m) => match m.type_().as_str() {
            "image" => Kind::Image,
            "video" => Kind::Video,
            "audio" => Kind::Audio,
            _ => Kind::Other,
        },
        None => Kind::Other,
    }
}

struct Entry {
    name: String,
    is_dir: bool,
    kind: Kind,
}

const SEG: &AsciiSet = &NON_ALPHANUMERIC.remove(b'-').remove(b'.').remove(b'_').remove(b'~');

fn enc(s: &str) -> String {
    utf8_percent_encode(s, SEG).to_string()
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// URL for a list of segments, e.g. ["a","b"] -> "/a/b".
fn url(segs: &[&str]) -> String {
    let mut u = String::new();
    for s in segs {
        u.push('/');
        u.push_str(&enc(s));
    }
    if u.is_empty() {
        u.push('/');
    }
    u
}

/// Decode a request path into safe segments; rejects `..`, absolute and empty-escape tricks.
fn segments(uri_path: &str) -> Option<Vec<String>> {
    let decoded = percent_decode_str(uri_path).decode_utf8().ok()?;
    let mut out = Vec::new();
    for c in Path::new(decoded.as_ref()).components() {
        match c {
            Component::Normal(s) => out.push(s.to_str()?.to_owned()),
            Component::RootDir | Component::CurDir => {}
            _ => return None,
        }
    }
    Some(out)
}

fn list_dir(dir: &Path) -> std::io::Result<Vec<Entry>> {
    let mut v: Vec<Entry> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let is_dir = e.path().is_dir(); // follows symlinks
            let kind = if is_dir { Kind::Other } else { kind_of(Path::new(&name)) };
            Entry { name, is_dir, kind }
        })
        .collect();
    v.sort_by(|a, b| {
        b.is_dir.cmp(&a.is_dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(v)
}

const CSS: &str = "
*{box-sizing:border-box}
body{margin:0;font:15px system-ui,sans-serif;background:#111;color:#ddd}
a{color:inherit;text-decoration:none}
header{display:flex;gap:.5em;align-items:center;padding:.7em 1em;background:#1b1b1b;position:sticky;top:0;z-index:1;flex-wrap:wrap}
header a{color:#6cf}
header .sp{flex:1}
.grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(180px,1fr));gap:12px;padding:12px}
.card{background:#1b1b1b;border-radius:8px;overflow:hidden;display:flex;flex-direction:column}
.card:hover{background:#262626}
.thumb{height:140px;display:flex;align-items:center;justify-content:center;font-size:48px;background:#0c0c0c}
.thumb img,.thumb video{width:100%;height:100%;object-fit:cover}
.name{padding:.5em .7em;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.stage{height:calc(100vh - 46px);display:flex;align-items:center;justify-content:center}
.stage img,.stage video{max-width:100%;max-height:100%}
.stage audio{width:80%}
";

fn page(title: &str, body: &str) -> Html<String> {
    Html(format!(
        "<!doctype html><meta charset=utf-8><meta name=viewport content=\"width=device-width,initial-scale=1\"><title>{}</title><style>{CSS}</style>{body}",
        esc(title)
    ))
}

fn breadcrumbs(segs: &[String]) -> String {
    let mut s = String::from("<a href=\"/\">/</a>");
    for i in 0..segs.len() {
        let parts: Vec<&str> = segs[..=i].iter().map(String::as_str).collect();
        let _ = write!(s, " <a href=\"{}/\">{}</a> /", url(&parts), esc(&segs[i]));
    }
    s
}

fn render_dir(segs: &[String], entries: &[Entry]) -> Html<String> {
    let base: Vec<&str> = segs.iter().map(String::as_str).collect();
    let mut b = format!("<header>{}</header><div class=grid>", breadcrumbs(segs));
    if !segs.is_empty() {
        let up = url(&base[..base.len() - 1]);
        let up = if up == "/" { up } else { format!("{up}/") };
        let _ = write!(b, "<a class=card href=\"{up}\"><div class=thumb>⬆️</div><div class=name>..</div></a>");
    }
    for e in entries {
        let mut p = base.clone();
        p.push(&e.name);
        let raw = url(&p);
        let (href, thumb) = if e.is_dir {
            (format!("{raw}/"), "📁".to_string())
        } else {
            match e.kind {
                Kind::Image => (format!("{raw}?view"), format!("<img loading=lazy src=\"{raw}\">")),
                Kind::Video => (
                    format!("{raw}?view"),
                    format!("<video preload=metadata muted src=\"{raw}#t=0.1\"></video>"),
                ),
                Kind::Audio => (format!("{raw}?view"), "🎵".into()),
                Kind::Other => (raw, "📄".into()),
            }
        };
        let _ = write!(
            b,
            "<a class=card href=\"{href}\"><div class=thumb>{thumb}</div><div class=name title=\"{n}\">{n}</div></a>",
            n = esc(&e.name)
        );
    }
    b.push_str("</div>");
    page(&format!("/{}", segs.join("/")), &b)
}

fn render_viewer(segs: &[String], siblings: &[Entry]) -> Html<String> {
    let (name, dir) = segs.split_last().expect("file has a name");
    let dir_p: Vec<&str> = dir.iter().map(String::as_str).collect();
    let mut file_p = dir_p.clone();
    file_p.push(name);
    let raw = url(&file_p);

    let media: Vec<&Entry> = siblings.iter().filter(|e| !e.is_dir && e.kind != Kind::Other).collect();
    let idx = media.iter().position(|e| &e.name == name);
    let link = |e: &Entry| {
        let mut p = dir_p.clone();
        p.push(&e.name);
        format!("{}?view", url(&p))
    };
    let prev = idx.and_then(|i| i.checked_sub(1)).map(|i| link(media[i]));
    let next = idx.and_then(|i| media.get(i + 1)).map(|e| link(e));
    let up = if dir_p.is_empty() { "/".to_string() } else { format!("{}/", url(&dir_p)) };

    let nav = |label: &str, id: &str, href: &Option<String>| match href {
        Some(h) => format!("<a id={id} href=\"{h}\">{label}</a>"),
        None => format!("<span style=\"opacity:.3\">{label}</span>"),
    };
    let el = match kind_of(Path::new(name)) {
        Kind::Image => format!("<img src=\"{raw}\">"),
        Kind::Video => format!("<video controls autoplay src=\"{raw}\"></video>"),
        Kind::Audio => format!("<audio controls autoplay src=\"{raw}\"></audio>"),
        Kind::Other => format!("<a href=\"{raw}\">Download</a>"),
    };
    let pos = idx.map(|i| format!("{}/{}", i + 1, media.len())).unwrap_or_default();
    let b = format!(
        "<header><a id=up href=\"{up}\">✕ back</a>{p} <span class=sp>{n} <small>{pos}</small></span>{nx} <a href=\"{raw}\" download>⬇</a></header>\
         <div class=stage>{el}</div>\
         <script>addEventListener('keydown',e=>{{if(e.target.tagName=='INPUT'||e.altKey||e.ctrlKey||e.metaKey)return;\
         const k={{ArrowLeft:'prev',ArrowRight:'next',Escape:'up'}}[e.key];const a=k&&document.getElementById(k);if(a)location=a.href}})</script>",
        p = nav("◀ prev", "prev", &prev),
        nx = nav("next ▶", "next", &next),
        n = esc(name),
    );
    page(name, &b)
}

async fn handle(State(root): State<Arc<PathBuf>>, req: Request) -> Response {
    let Some(segs) = segments(req.uri().path()) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let mut path = (*root).clone();
    path.extend(&segs);

    if path.is_dir() {
        return match list_dir(&path) {
            Ok(e) => render_dir(&segs, &e).into_response(),
            Err(_) => StatusCode::FORBIDDEN.into_response(),
        };
    }
    if !path.is_file() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let wants_view = req.uri().query().is_some_and(|q| q.split('&').any(|p| p == "view"));
    if wants_view {
        let parent = path.parent().unwrap_or(&root);
        return match list_dir(parent) {
            Ok(e) => render_viewer(&segs, &e).into_response(),
            Err(_) => StatusCode::FORBIDDEN.into_response(),
        };
    }
    // ServeFile handles Range requests (video seeking), content-type and caching headers.
    match ServeFile::new(path).oneshot(req).await {
        Ok(r) => r.into_response(),
        Err(e) => match e {},
    }
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let root = std::fs::canonicalize(&args.dir).unwrap_or_else(|e| {
        eprintln!("{}: {e}", args.dir.display());
        std::process::exit(1)
    });
    if !root.is_dir() {
        eprintln!("{}: not a directory", root.display());
        std::process::exit(1)
    }
    let app = Router::new().fallback(handle).with_state(Arc::new(root.clone()));
    let addr = format!("{}:{}", args.bind, args.port);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap_or_else(|e| {
        eprintln!("bind {addr}: {e}");
        std::process::exit(1)
    });
    println!("serving {} on http://{addr}", root.display());
    axum::serve(listener, app).await.unwrap();
}
