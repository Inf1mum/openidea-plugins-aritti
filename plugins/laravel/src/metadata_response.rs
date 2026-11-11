//! Response header keys from the documented response helper/factory/constructor signatures.
use super::*;
// Header argument positions match Illuminate\Routing\ResponseFactory in 10.x and 13.x.
// https://raw.githubusercontent.com/laravel/framework/10.x/src/Illuminate/Routing/ResponseFactory.php
// IANA HTTP Field Name Registry, snapshot 2026-11-02.
// https://www.iana.org/assignments/http-fields/field-names.csv
const HEADERS: &[(&str, &str)] = &[
    ("A-IM", "permanent"),
    ("Accept", "permanent"),
    ("Accept-Additions", "permanent"),
    ("Accept-CH", "permanent"),
    ("Accept-Charset", "deprecated"),
    ("Accept-Datetime", "permanent"),
    ("Accept-Encoding", "permanent"),
    ("Accept-Features", "permanent"),
    ("Accept-Language", "permanent"),
    ("Accept-Patch", "permanent"),
    ("Accept-Post", "permanent"),
    ("Accept-Query", "permanent"),
    ("Accept-Ranges", "permanent"),
    ("Accept-Signature", "permanent"),
    ("Access-Control", "obsoleted"),
    ("Access-Control-Allow-Credentials", "permanent"),
    ("Access-Control-Allow-Headers", "permanent"),
    ("Access-Control-Allow-Methods", "permanent"),
    ("Access-Control-Allow-Origin", "permanent"),
    ("Access-Control-Expose-Headers", "permanent"),
    ("Access-Control-Max-Age", "permanent"),
    ("Access-Control-Request-Headers", "permanent"),
    ("Access-Control-Request-Method", "permanent"),
    ("Activate-Storage-Access", "provisional"),
    ("Age", "permanent"),
    ("Allow", "permanent"),
    ("ALPN", "permanent"),
    ("Alt-Svc", "permanent"),
    ("Alt-Used", "permanent"),
    ("Alternates", "permanent"),
    ("AMP-Cache-Transform", "provisional"),
    ("Apply-To-Redirect-Ref", "permanent"),
    ("Authentication-Control", "permanent"),
    ("Authentication-Info", "permanent"),
    ("Authorization", "permanent"),
    ("Available-Dictionary", "permanent"),
    ("C-Ext", "obsoleted"),
    ("C-Man", "obsoleted"),
    ("C-Opt", "obsoleted"),
    ("C-PEP", "obsoleted"),
    ("C-PEP-Info", "deprecated"),
    ("Cache-Control", "permanent"),
    ("Cache-Group-Invalidation", "permanent"),
    ("Cache-Groups", "permanent"),
    ("Cache-Status", "permanent"),
    ("Cal-Managed-ID", "permanent"),
    ("CalDAV-Timezones", "permanent"),
    ("Capsule-Protocol", "permanent"),
    ("CDN-Cache-Control", "permanent"),
    ("CDN-Loop", "permanent"),
    ("Cert-Not-After", "permanent"),
    ("Cert-Not-Before", "permanent"),
    ("Clear-Site-Data", "permanent"),
    ("Client-Cert", "permanent"),
    ("Client-Cert-Chain", "permanent"),
    ("CMCD-Object", "provisional"),
    ("CMCD-Request", "provisional"),
    ("CMCD-Session", "provisional"),
    ("CMCD-Status", "provisional"),
    ("CMSD-Dynamic", "provisional"),
    ("CMSD-Static", "provisional"),
    ("Concealed-Auth-Export", "permanent"),
    ("Configuration-Context", "provisional"),
    ("Connect-UDP-Bind", "permanent"),
    ("Connection", "permanent"),
    ("Content-Base", "obsoleted"),
    ("Content-Digest", "permanent"),
    ("Content-Disposition", "permanent"),
    ("Content-Encoding", "permanent"),
    ("Content-ID", "deprecated"),
    ("Content-Language", "permanent"),
    ("Content-Length", "permanent"),
    ("Content-Location", "permanent"),
    ("Content-MD5", "obsoleted"),
    ("Content-Range", "permanent"),
    ("Content-Script-Type", "obsoleted"),
    ("Content-Security-Policy", "permanent"),
    ("Content-Security-Policy-Report-Only", "permanent"),
    ("Content-Style-Type", "obsoleted"),
    ("Content-Type", "permanent"),
    ("Content-Version", "obsoleted"),
    ("Cookie", "permanent"),
    ("Cookie2", "obsoleted"),
    ("Cross-Origin-Embedder-Policy", "permanent"),
    ("Cross-Origin-Embedder-Policy-Report-Only", "permanent"),
    ("Cross-Origin-Opener-Policy", "permanent"),
    ("Cross-Origin-Opener-Policy-Report-Only", "permanent"),
    ("Cross-Origin-Resource-Policy", "permanent"),
    ("CTA-Common-Access-Token", "provisional"),
    ("DASL", "permanent"),
    ("Date", "permanent"),
    ("DAV", "permanent"),
    ("Default-Style", "obsoleted"),
    ("Delta-Base", "permanent"),
    ("Deprecation", "permanent"),
    ("Depth", "permanent"),
    ("Derived-From", "obsoleted"),
    ("Destination", "permanent"),
    ("Detached-JWS", "permanent"),
    ("Differential-ID", "deprecated"),
    ("Dictionary-ID", "permanent"),
    ("Digest", "obsoleted"),
    ("DPoP", "permanent"),
    ("DPoP-Nonce", "permanent"),
    ("Early-Data", "permanent"),
    ("EDIINT-Features", "provisional"),
    ("ETag", "permanent"),
    ("Expect", "permanent"),
    ("Expect-CT", "deprecated"),
    ("Expires", "permanent"),
    ("Ext", "obsoleted"),
    ("Forwarded", "permanent"),
    ("From", "permanent"),
    ("GetProfile", "obsoleted"),
    ("Hobareg", "permanent"),
    ("Host", "permanent"),
    ("HTTP2-Settings", "obsoleted"),
    ("If", "permanent"),
    ("If-Match", "permanent"),
    ("If-Modified-Since", "permanent"),
    ("If-None-Match", "permanent"),
    ("If-Range", "permanent"),
    ("If-Schedule-Tag-Match", "permanent"),
    ("If-Unmodified-Since", "permanent"),
    ("IM", "permanent"),
    ("Include-Referred-Token-Binding-ID", "permanent"),
    ("Incremental", "permanent"),
    ("Isolation", "provisional"),
    ("Keep-Alive", "permanent"),
    ("Label", "permanent"),
    ("Last-Event-ID", "permanent"),
    ("Last-Modified", "permanent"),
    ("Link", "permanent"),
    ("Link-Template", "permanent"),
    ("Location", "permanent"),
    ("Lock-Token", "permanent"),
    ("Man", "obsoleted"),
    ("Max-Forwards", "permanent"),
    ("Memento-Datetime", "permanent"),
    ("Meter", "permanent"),
    ("Method-Check", "obsoleted"),
    ("Method-Check-Expires", "obsoleted"),
    ("MIME-Version", "permanent"),
    ("Negotiate", "permanent"),
    ("NEL", "permanent"),
    ("No-Vary-Search", "permanent"),
    ("OData-EntityId", "permanent"),
    ("OData-Isolation", "permanent"),
    ("OData-MaxVersion", "permanent"),
    ("OData-Version", "permanent"),
    ("Opt", "obsoleted"),
    ("Optional-WWW-Authenticate", "permanent"),
    ("Ordering-Type", "permanent"),
    ("Origin", "permanent"),
    ("Origin-Agent-Cluster", "permanent"),
    ("OSCORE", "permanent"),
    ("OSLC-Core-Version", "permanent"),
    ("Overwrite", "permanent"),
    ("P3P", "obsoleted"),
    ("PEP", "obsoleted"),
    ("PEP-Info", "obsoleted"),
    ("Permissions-Policy", "provisional"),
    ("PICS-Label", "obsoleted"),
    ("Ping-From", "permanent"),
    ("Ping-To", "permanent"),
    ("Position", "permanent"),
    ("Pragma", "deprecated"),
    ("Prefer", "permanent"),
    ("Preference-Applied", "permanent"),
    ("Priority", "permanent"),
    ("ProfileObject", "obsoleted"),
    ("Protocol", "obsoleted"),
    ("Protocol-Info", "deprecated"),
    ("Protocol-Query", "deprecated"),
    ("Protocol-Request", "obsoleted"),
    ("Proxy-Authenticate", "permanent"),
    ("Proxy-Authentication-Info", "permanent"),
    ("Proxy-Authorization", "permanent"),
    ("Proxy-Features", "obsoleted"),
    ("Proxy-Instruction", "obsoleted"),
    ("Proxy-Public-Address", "permanent"),
    ("Proxy-Status", "permanent"),
    ("Public", "obsoleted"),
    ("Public-Key-Pins", "permanent"),
    ("Public-Key-Pins-Report-Only", "permanent"),
    ("Range", "permanent"),
    ("Redirect-Ref", "permanent"),
    ("Referer", "permanent"),
    ("Referer-Root", "obsoleted"),
    ("Referrer-Policy", "permanent"),
    ("Refresh", "permanent"),
    ("Repeatability-Client-ID", "provisional"),
    ("Repeatability-First-Sent", "provisional"),
    ("Repeatability-Request-ID", "provisional"),
    ("Repeatability-Result", "provisional"),
    ("Replay-Nonce", "permanent"),
    ("Reporting-Endpoints", "provisional"),
    ("Repr-Digest", "permanent"),
    ("Retry-After", "permanent"),
    ("Safe", "obsoleted"),
    ("Schedule-Reply", "permanent"),
    ("Schedule-Tag", "permanent"),
    ("Sec-Fetch-Dest", "Permanent"),
    ("Sec-Fetch-Mode", "Permanent"),
    ("Sec-Fetch-Site", "Permanent"),
    ("Sec-Fetch-Storage-Access", "provisional"),
    ("Sec-Fetch-User", "Permanent"),
    ("Sec-GPC", "provisional"),
    ("Sec-Purpose", "permanent"),
    ("Sec-Token-Binding", "permanent"),
    ("Sec-WebSocket-Accept", "permanent"),
    ("Sec-WebSocket-Extensions", "permanent"),
    ("Sec-WebSocket-Key", "permanent"),
    ("Sec-WebSocket-Protocol", "permanent"),
    ("Sec-WebSocket-Version", "permanent"),
    ("Security-Scheme", "obsoleted"),
    ("Server", "permanent"),
    ("Server-Timing", "permanent"),
    ("Set-Cookie", "permanent"),
    ("Set-Cookie2", "obsoleted"),
    ("Set-Txn", "permanent"),
    ("SetProfile", "obsoleted"),
    ("Signature", "permanent"),
    ("Signature-Input", "permanent"),
    ("SLUG", "permanent"),
    ("SoapAction", "permanent"),
    ("Status-URI", "permanent"),
    ("Strict-Transport-Security", "permanent"),
    ("Sunset", "permanent"),
    ("Surrogate-Capability", "provisional"),
    ("Surrogate-Control", "provisional"),
    ("TCN", "permanent"),
    ("TE", "permanent"),
    ("Timeout", "permanent"),
    ("Timing-Allow-Origin", "provisional"),
    ("Topic", "permanent"),
    ("Traceparent", "permanent"),
    ("Tracestate", "permanent"),
    ("Trailer", "permanent"),
    ("Transfer-Encoding", "permanent"),
    ("TTL", "permanent"),
    ("Unencoded-Digest", "permanent"),
    ("Upgrade", "permanent"),
    ("Urgency", "permanent"),
    ("URI", "obsoleted"),
    ("Use-As-Dictionary", "permanent"),
    ("User-Agent", "permanent"),
    ("Variant-Vary", "permanent"),
    ("Vary", "permanent"),
    ("Via", "permanent"),
    ("Want-Content-Digest", "permanent"),
    ("Want-Digest", "obsoleted"),
    ("Want-Repr-Digest", "permanent"),
    ("Want-Unencoded-Digest", "permanent"),
    ("Warning", "obsoleted"),
    ("WWW-Authenticate", "permanent"),
    ("X-Content-Type-Options", "permanent"),
    ("X-Frame-Options", "permanent"),
];
pub(super) fn completion(
    index: &Index,
    doc: &openidea_plugin::providers::Document,
    at: usize,
) -> Vec<CompletionItem> {
    if !index.metadata.project_root.is_empty()
        && host::project_of(&doc.path)
            .and_then(|owner| owner.root().ok())
            .as_deref()
            != Some(index.metadata.project_root.as_str())
    {
        return Vec::new();
    }
    let Some(site) = crate::eloquent_members::metadata_inference::completion_site(&doc.text, at)
    else {
        return Vec::new();
    };
    if site.return_context
        || site.pipe
            && index
                .metadata
                .php_version
                .is_some_and(|version| version < (8, 5, 0))
    {
        return Vec::new();
    }
    let identity = &site.identity;
    let response_class = |class: &str| {
        [
            "Illuminate\\Http\\Response",
            "Illuminate\\Http\\JsonResponse",
            "Illuminate\\Http\\RedirectResponse",
            "Symfony\\Component\\HttpFoundation\\Response",
            "Symfony\\Component\\HttpFoundation\\JsonResponse",
            "Symfony\\Component\\HttpFoundation\\RedirectResponse",
        ]
        .iter()
        .any(|expected| class.eq_ignore_ascii_case(expected))
    };
    let method = identity.method.to_ascii_lowercase();
    let factory_method = |method: &str| match method {
        "make" | "json" | "stream" | "streamdownload" | "download" | "redirectto"
        | "redirectguest" | "redirecttointended" => Some(3),
        "nocontent" | "file" => Some(2),
        "view" | "jsonp" | "redirecttoroute" | "redirecttoaction" => Some(4),
        "streamjson"
            if index
                .metadata
                .framework_version
                .is_some_and(|version| version.0 >= 11) =>
        {
            Some(3)
        }
        "eventstream"
            if index
                .metadata
                .framework_version
                .is_some_and(|version| version.0 >= 12) =>
        {
            Some(2)
        }
        "markdown"
            if index
                .metadata
                .framework_version
                .is_some_and(|version| version.0 >= 13) =>
        {
            Some(3)
        }
        _ => None,
    };
    let (parameter, direct) = if identity.constructor {
        if !identity.class.as_deref().is_some_and(response_class) {
            return Vec::new();
        }
        (3, false)
    } else if let Some(class) = identity.class.as_deref() {
        let factory = class.eq_ignore_ascii_case("Illuminate\\Support\\Facades\\Response")
            || class.eq_ignore_ascii_case("Illuminate\\Routing\\ResponseFactory")
            || class.eq_ignore_ascii_case("Illuminate\\Contracts\\Routing\\ResponseFactory")
            || class.eq_ignore_ascii_case("Response")
                && !crate::php::declares_class(&doc.text, "Response");
        if factory {
            let Some(parameter) = factory_method(&method) else {
                return Vec::new();
            };
            (parameter, false)
        } else if response_class(class) && method == "withheaders" {
            (1, false)
        } else if response_class(class) && method == "header" {
            (1, true)
        } else {
            return Vec::new();
        }
    } else if !site.unresolved
        && crate::eloquent_members::metadata_inference::global_helper(
            &doc.text, site.open, "response",
        )
    {
        (3, false)
    } else {
        match crate::eloquent_members::metadata_inference::response_receiver(&doc.text, site.open) {
            Some(true) => {
                let Some(parameter) = factory_method(&method) else {
                    return Vec::new();
                };
                (parameter, false)
            }
            Some(false) if method == "withheaders" => (1, false),
            Some(false) if method == "header" => (1, true),
            _ => return Vec::new(),
        }
    };
    let parameter_name = if direct { "key" } else { "headers" };
    if site
        .parameter_name
        .as_deref()
        .map_or(site.argument != parameter, |name| name != parameter_name)
    {
        return Vec::new();
    }
    if direct {
        if site.pipe || !site.arrays.is_empty() {
            return Vec::new();
        }
    } else if site.arrays.len() != 1 || site.arrays[0].has_key && !site.arrays[0].is_key {
        return Vec::new();
    }
    HEADERS
        .iter()
        .map(|(name, status)| {
            let mut item = CompletionItem::new(name, CompletionKind::Property);
            item.detail = Some(crate::ui_text::format("metadata.detail.httpHeader","HTTP response header · IANA {status}",&[("status",status)]));
            let mut end = site.literal.end;
            if !direct && !site.arrays[0].has_key {
                end += usize::from(site.closed);
                item.insert_text = format!("{name}{} => $0", char::from(site.quote));
                item.snippet = true;
            }
            item.replace = Some(TextRange::new(
                text::position(&doc.text, site.literal.start),
                text::position(&doc.text, end),
            ));
            item
        })
        .collect()
}
