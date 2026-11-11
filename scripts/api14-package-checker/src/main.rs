//! Released-host manifest and actual component linker validation, without GUI dependencies.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let directory = args.next().ok_or("usage: openidea-parity-package-checker PACKAGE [FRESH_WASM]")?;
    let directory = std::path::PathBuf::from(directory);
    let manifest = ide_extension::Manifest::load(&directory)?;
    let wasm = args.next().map(std::path::PathBuf::from)
        .or_else(|| manifest.wasm.as_ref().map(|p| directory.join(p)))
        .ok_or("package does not declare Wasm")?;
    if args.next().is_some() { return Err("unexpected argument".into()); }
    let engine = ide_plugin::PluginEngine::new()?;
    let component = engine.compile(&wasm)?;
    let version = engine.check_component(&component)?;
    // The host's base check permits optional interfaces to be absent. A plugin
    // declaring a provider must actually export the interface that dispatches it.
    let exports: Vec<String> = component.component_type().exports(engine.engine())
        .map(|(name,_)|name.to_owned()).collect();
    let mut required = std::collections::BTreeSet::new();
    let mut code_actions = false;
    if let Some(providers)=manifest.contributes.get("providers").and_then(|v|v.as_array()) {
        for provider in providers {
            if let Some(kinds)=provider.get("kinds").and_then(|v|v.as_array()) {
                for kind in kinds.iter().filter_map(|v|v.as_str()) {
                    let interface=match kind {
                        "completion"|"inline-completion"|"definition"|"line-markers"=>"providers",
                        "code-actions"=>{code_actions=true;continue;},
                        "diagnostics"=>"diagnostics-provider",
                        "hover"=>"hover-provider",
                        "semantic-tokens"=>"semantic-tokens-provider",
                        "inlay-hints"=>"inlay-hints-provider",
                        "injections"=>"injections",
                        "code-lenses"|"document-symbols"|"formatting"|"signature-help"|"document-highlights"|"document-links"|"selection-ranges"|"folding"|"colors"|"implementation"|"type-definition"|"gutter-actions"=>"language-features",
                        _=>return Err(format!("checker lacks a dispatch mapping for declared provider kind {kind}").into()),
                    };
                    required.insert(interface);
                }
            }
        }
    }
    for (contribution,interface) in [("documentIndexers","document-indexer"),("endpoints","endpoints-provider"),("commitChecks","commit-checker"),("runConfigurations","run-configurations"),("views","view-provider")] {
        if manifest.contributes.get(contribution).and_then(|v|v.as_array()).is_some_and(|items|!items.is_empty()) {required.insert(interface);}
    }
    if code_actions && !exports.iter().any(|name|name.starts_with("openidea:plugin/providers@")||name.starts_with("openidea:plugin/file-actions@")) {
        return Err(format!("{} declares code actions but exports neither providers nor file-actions",manifest.id).into());
    }
    for interface in &required {
        let prefix=format!("openidea:plugin/{interface}@");
        if !exports.iter().any(|name|name.starts_with(&prefix)) {
            return Err(format!("{} declares {interface} behavior but its actual Wasm has no {interface} export",manifest.id).into());
        }
    }
    println!("PASS {} declared optional dispatch interfaces",required.len());
    println!("PASS manifest {}; component {}; guest {version:?}; host API {}", manifest.id, wasm.display(), ide_plugin::API_VERSION);
    Ok(())
}
