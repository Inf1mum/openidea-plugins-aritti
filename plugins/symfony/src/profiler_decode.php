// Read-only adapter: no application bootstrap, autoload or executable profile objects.
error_reporting(0);
try {
    $root = realpath($argv[1]);
    $file = realpath($argv[2]);
    if (!$root || !$file || !str_starts_with($file, $root.DIRECTORY_SEPARATOR) || !is_file($file)) { exit(2); }
    $h = fopen($file, 'rb');
    if (!$h) { exit(2); }
    $raw = stream_get_contents($h, 1048577);
    fclose($h);
    if (strlen($raw) > 1048576) { exit(3); }
    if (str_starts_with($raw, "\x1f\x8b")) {
        if (!function_exists('gzdecode')) { exit(4); }
        $raw = gzdecode($raw, 1048576);
        if (!is_string($raw)) { exit(3); }
    }
    $value = unserialize($raw, ['allowed_classes' => false, 'max_depth' => 32]);
    if (!is_array($value) || ($value['token'] ?? null) !== $argv[3] || !is_array($value['data'] ?? null) || count($value['data']) > 128) { exit(5); }
    $rows = [];
    $keys = ['time', 'duration', 'memory', 'memory_limit', 'peak_memory', 'query_count', 'query_time', 'error_count', 'exception_count', 'status_code'];
    foreach ($value['data'] as $name => $collector) {
        if (!is_string($name) || !preg_match('/^[a-zA-Z0-9_.-]{1,80}$/D', $name)) { continue; }
        $fields = (array) $collector;
        $class = $fields['__PHP_Incomplete_Class_Name'] ?? (is_array($collector) ? 'array' : 'unknown');
        if (!is_string($class) || !preg_match('/^[a-zA-Z0-9_\\\\]{1,200}$/D', $class)) { $class = 'unknown'; }
        $data = $fields;
        foreach ($fields as $key => $field) {
            if (is_string($key) && (substr($key, strrpos($key, "\0") !== false ? strrpos($key, "\0") + 1 : 0) === 'data') && is_array($field)) { $data = $field; break; }
        }
        $metrics = [];
        foreach ($keys as $key) {
            $n = $data[$key] ?? null;
            if ((is_int($n) || is_float($n)) && is_finite((float) $n) && $n >= 0 && $n <= 1.0e15) { $metrics[$key] = $n; }
        }
        foreach (['logs', 'exceptions', 'templates', 'events'] as $field) {
            if (is_array($data[$field] ?? null)) { $metrics['stored_'.$field] = count($data[$field]); }
        }
        if (!isset($metrics['query_count']) && is_array($data['queries'] ?? null)) {
            $count = 0;
            foreach ($data['queries'] as $queries) {
                if (is_array($queries)) { $count += array_key_exists('sql', $queries) ? 1 : count($queries); }
            }
            $metrics['query_count'] = $count;
        }
        $rows[] = ['name' => $name, 'class' => $class, 'metrics' => (object) $metrics];
    }
    echo json_encode(['version' => 1, 'token' => $argv[3], 'collectors' => $rows], JSON_THROW_ON_ERROR);
} catch (Throwable $e) { exit(5); }
