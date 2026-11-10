<?php
// Explicit opt-in runtime inspection. No file installation, shell, job execution or payload reads.
$mode = $argv[1] ?? '';
$options = json_decode($argv[2] ?? '{}', true, 16, JSON_THROW_ON_ERROR);
if ($mode === 'logs') {
    $root = realpath(getcwd());
    $relative = $options['file'] ?? '';
    if (!is_string($relative) || strlen($relative) > 1024 || !str_starts_with($relative, 'storage/logs/') || !str_ends_with($relative, '.log') || preg_match('~(?:^|/)\.\.?(?:/|$)|[\x00-\x1f]~', $relative)) {
        throw new RuntimeException('Select one project storage/logs/*.log file.');
    }
    $path = realpath($root.'/'.$relative);
    $directory = realpath($root.'/storage/logs');
    if ($path === false || $directory === false || !str_starts_with($path, $directory.'/') || !str_starts_with($path, $root.'/')) {
        throw new RuntimeException('Log path escapes the project log directory.');
    }
    $stream = fopen($path, 'rb');
    if ($stream === false) { throw new RuntimeException('Selected log is unavailable.'); }
    $size = fstat($stream)['size'];
    $offset = max(0, $size - 65536);
    fseek($stream, $offset);
    $tail = fread($stream, 65536);
    fclose($stream);
    if ($tail === false) { throw new RuntimeException('Selected log is unreadable.'); }
    if ($offset > 0) { $newline = strpos($tail, "\n"); $tail = $newline === false ? '' : substr($tail, $newline + 1); }
    $lines = explode("\n", rtrim($tail, "\r\n"));
    if ($tail === '') { $lines = []; }
    echo json_encode(['file' => $relative, 'truncated' => $offset > 0 || count($lines) > 200, 'lines' => array_slice($lines, -200)], JSON_THROW_ON_ERROR | JSON_INVALID_UTF8_SUBSTITUTE);
    exit;
}
require getcwd().'/vendor/autoload.php';
$app = require getcwd().'/bootstrap/app.php';
$app->make(\Illuminate\Contracts\Console\Kernel::class)->bootstrap();
if ($mode === 'queue-status') {
    $connections = $app['config']->get('queue.connections', []);
    if (!is_array($connections) || count($connections) > 32) {
        throw new RuntimeException('Queue connection inventory exceeds 32 entries.');
    }
    $selected = $options['connection'] ?? null;
    if ($selected !== null && !array_key_exists($selected, $connections)) {
        throw new RuntimeException('Selected queue connection is not configured.');
    }
    $rows = [];
    foreach ($connections as $name => $config) {
        if ($selected !== null && $name !== $selected) { continue; }
        $driver = is_array($config) ? ($config['driver'] ?? '') : '';
        $queue = $options['queue'] ?? ($config['queue'] ?? 'default');
        if (!is_string($name) || strlen($name) > 128 || !is_string($driver) || strlen($driver) > 64 || !is_string($queue) || strlen($queue) > 256) {
            throw new RuntimeException('Unsupported queue connection metadata.');
        }
        $row = ['connection' => $name, 'driver' => $driver, 'queue' => $queue,
            'total' => null, 'pending' => null, 'reserved' => null, 'delayed' => null,
            'running' => null, 'status' => 'unsupported',
            'note' => 'This driver does not expose persistent queue statistics.'];
        if (in_array($driver, ['database', 'redis', 'sqs', 'beanstalkd'], true)) {
            try {
                $connection = $app['queue']->connection($name);
                $known = $connection instanceof \Illuminate\Queue\DatabaseQueue
                    || $connection instanceof \Illuminate\Queue\RedisQueue
                    || $connection instanceof \Illuminate\Queue\SqsQueue
                    || $connection instanceof \Illuminate\Queue\BeanstalkdQueue;
                if (!$known) { throw new RuntimeException('Custom queue adapters are not inspected.'); }
                $complete = true;
                foreach (['total' => 'size', 'pending' => 'pendingSize', 'reserved' => 'reservedSize', 'delayed' => 'delayedSize'] as $field => $method) {
                    if (!is_callable([$connection, $method])) { $complete = false; continue; }
                    $count = $connection->$method($queue);
                    if (!is_int($count) || $count < 0) { $complete = false; continue; }
                    $row[$field] = $count;
                }
                $row['status'] = $complete ? 'available' : 'partial';
                $row['note'] = 'Reserved jobs may be running or awaiting reservation expiry; running job count is unavailable.';
            } catch (Throwable $error) {
                // Driver errors often include credentials/DSNs. Report the type, never the message.
                $row['status'] = 'unavailable';
                $row['note'] = 'Queue statistics unavailable ('.get_class($error).').';
            }
        }
        $rows[] = $row;
    }
    echo json_encode(['queues' => $rows], JSON_THROW_ON_ERROR | JSON_INVALID_UTF8_SUBSTITUTE);
} elseif ($mode === 'workers') {
    if (!is_dir('/proc') || !is_readable('/proc/self/cmdline')) {
        echo json_encode(['status' => 'unavailable', 'note' => 'Worker process visibility requires a readable Linux /proc in the selected PHP runner.', 'workers' => []], JSON_THROW_ON_ERROR);
        exit;
    }
    $root = realpath(getcwd());
    $paths = glob('/proc/[0-9]*/cmdline') ?: [];
    $workers = [];
    $skipped = 0;
    $limited = count($paths) > 4096;
    foreach (array_slice($paths, 0, 4096) as $path) {
        $stream = @fopen($path, 'rb');
        if ($stream === false) { $skipped++; continue; }
        $command = fread($stream, 8193);
        fclose($stream);
        if ($command === false || strlen($command) > 8192) { $skipped++; continue; }
        $arguments = explode("\0", $command);
        $kind = null;
        foreach (['queue:work', 'queue:listen', 'horizon', 'horizon:work', 'horizon:supervisor'] as $candidate) {
            if (in_array($candidate, $arguments, true)) { $kind = $candidate; break; }
        }
        if ($kind === null || !preg_match('/^php(?:[0-9]+(?:\.[0-9]+)*)?$/', basename($arguments[0] ?? ''))) { continue; }
        $pid = (int) basename(dirname($path));
        $cwd = @readlink('/proc/'.$pid.'/cwd');
        $artisan = null;
        foreach ($arguments as $argument) {
            if (basename($argument) === 'artisan') { $artisan = $argument; break; }
        }
        if ($artisan === null || !is_string($cwd)) { $skipped++; continue; }
        $entry = str_starts_with($artisan, '/') ? $artisan : $cwd.'/'.$artisan;
        if (realpath($entry) !== $root.'/artisan') { continue; }
        $workers[] = ['pid' => $pid, 'command' => $kind];
        if (count($workers) >= 256) { $limited = true; break; }
    }
    echo json_encode(['status' => $limited || $skipped > 0 ? 'incomplete' : 'available',
        'note' => 'Observed project Artisan worker processes only; process presence does not prove a job is running.'.($limited || $skipped > 0 ? ' Some process metadata was unavailable or the scan limit was reached.' : ''),
        'workers' => $workers], JSON_THROW_ON_ERROR);
} else {
    throw new RuntimeException('Unsupported inspection mode.');
}
