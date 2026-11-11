//! Laravel snippets for PHP files, offered where a statement or class member starts.

/// `(label, description, body, imports)` in snippet syntax; a literal `$` is written `\$`.
const SNIPPETS: &[(&str, &str, &str, &[&str])] = &[
    ("routeget", "Route::get", "Route::get('${1:/}', [${2:Controller}::class, '${3:index}'])->name('${4:name}');", &["Illuminate\\Support\\Facades\\Route"]),
    ("routepost", "Route::post", "Route::post('${1:/}', [${2:Controller}::class, '${3:store}'])->name('${4:name}');", &["Illuminate\\Support\\Facades\\Route"]),
    ("routeresource", "Route::resource", "Route::resource('${1:photos}', ${2:PhotoController}::class);", &["Illuminate\\Support\\Facades\\Route"]),
    ("routeapiresource", "Route::apiResource", "Route::apiResource('${1:photos}', ${2:PhotoController}::class);", &["Illuminate\\Support\\Facades\\Route"]),
    ("routegroup", "Route::middleware()->group", "Route::middleware('${1:auth}')->group(function () {\n\t$0\n});", &["Illuminate\\Support\\Facades\\Route"]),
    ("dd", "dd()", "dd(${1:\\$value});", &[]),
    ("ddd", "dump and die with a trace", "ddd(${1:\\$value});", &[]),
    ("logi", "Log::info", "Log::info('${1:message}', [${2}]);", &["Illuminate\\Support\\Facades\\Log"]),
    ("loge", "Log::error", "Log::error('${1:message}', [${2}]);", &["Illuminate\\Support\\Facades\\Log"]),
    ("cacheremember", "Cache::remember", "Cache::remember('${1:key}', ${2:now()->addHour()}, fn () => ${3:value});", &["Illuminate\\Support\\Facades\\Cache"]),
    ("dbtransaction", "DB::transaction", "DB::transaction(function () {\n\t$0\n});", &["Illuminate\\Support\\Facades\\DB"]),
    ("validate", "\\$request->validate", "\\$request->validate([\n\t'${1:field}' => ['required'],\n]);", &[]),
    ("fillable", "protected \\$fillable", "protected \\$fillable = [\n\t'${1:name}',\n];", &[]),
    ("casts", "casts() method", "protected function casts(): array\n{\n\treturn [\n\t\t'${1:column}' => '${2:datetime}',\n\t];\n}", &[]),
    ("hasmany", "hasMany relation", "public function ${1:posts}(): HasMany\n{\n\treturn \\$this->hasMany(${2:Post}::class);\n}", &["Illuminate\\Database\\Eloquent\\Relations\\HasMany"]),
    ("belongsto", "belongsTo relation", "public function ${1:user}(): BelongsTo\n{\n\treturn \\$this->belongsTo(${2:User}::class);\n}", &["Illuminate\\Database\\Eloquent\\Relations\\BelongsTo"]),
    ("belongstomany", "belongsToMany relation", "public function ${1:roles}(): BelongsToMany\n{\n\treturn \\$this->belongsToMany(${2:Role}::class);\n}", &["Illuminate\\Database\\Eloquent\\Relations\\BelongsToMany"]),
    ("scope", "local scope", "#[Scope]\nprotected function ${1:active}(Builder \\$query): void\n{\n\t\\$query->where('${2:active}', true);\n}", &["Illuminate\\Database\\Eloquent\\Attributes\\Scope", "Illuminate\\Database\\Eloquent\\Builder"]),
    ("jobhandle", "job handle()", "public function handle(): void\n{\n\t$0\n}", &[]),
    ("policymethod", "policy method", "public function ${1:view}(User \\$user, ${2:Post} \\$${3:post}): bool\n{\n\treturn ${4:true};\n}", &[]),
    ("httpfake", "Http::fake", "Http::fake([\n\t'${1:*}' => Http::response(${2:[]}, ${3:200}),\n]);", &["Illuminate\\Support\\Facades\\Http"]),
    ("eventfake", "Event::fake", "Event::fake([${1:Event}::class]);", &["Illuminate\\Support\\Facades\\Event"]),
    ("queuefake", "Queue::fake", "Queue::fake();", &["Illuminate\\Support\\Facades\\Queue"]),
    ("mailfake", "Mail::fake", "Mail::fake();", &["Illuminate\\Support\\Facades\\Mail"]),
    ("redirectroute", "redirect()->route", "return redirect()->route('${1:name}');", &[]),
    ("abortif", "abort_if", "abort_if(${1:\\$condition}, ${2:403});", &[]),
];

/// The typed prefix before `offset` when it starts a statement or class member: `(start, prefix)`.
pub fn context(text: &str, offset: usize) -> Option<(usize, &str)> {
    let line_start = text[..offset].rfind('\n').map_or(0, |i| i + 1);
    let before = &text[line_start..offset];
    let prefix = before.trim_start();
    if prefix.len() < 2 || !prefix.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return None;
    }

    Some((offset - prefix.len(), prefix))
}

pub fn matching(prefix: &str) -> impl Iterator<Item = &'static (&'static str, &'static str, &'static str, &'static [&'static str])> {
    let prefix = prefix.to_ascii_lowercase();
    SNIPPETS.iter().filter(move |(label, ..)| label.starts_with(&prefix))
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_a_statement_start_prefix_matches() {
        assert_eq!(super::context("    rou", 7).map(|(_, p)| p), Some("rou"));
        assert!(super::context("    $x->rou", 11).is_none());
        assert!(super::context("    r", 5).is_none());
        assert_eq!(super::matching("routege").count(), 1);
        assert_eq!(super::matching("rout").count(), 4 + 1);
    }
}
