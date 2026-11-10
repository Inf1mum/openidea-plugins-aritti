
\Illuminate\Support\Facades\Artisan::command('workspace:probe {name} {tags?*} {--label=standard} {--flag} {--fail} {--bytes=0} {--wait=0}', function () {
    $this->line(json_encode(['name'=>$this->argument('name'),'tags'=>$this->argument('tags'),'label'=>$this->option('label'),'flag'=>$this->option('flag')], JSON_UNESCAPED_UNICODE|JSON_UNESCAPED_SLASHES));
    if ($this->option('bytes')) { echo str_repeat('x', (int) $this->option('bytes')); }
    if ($this->option('wait')) { sleep((int) $this->option('wait')); }
    if ($this->option('fail')) { $this->error('Expected acceptance failure'); return 7; }
    return 0;
});
