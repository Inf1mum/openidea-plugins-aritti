<?php
// Run: php emit-monolog-json.php /path/to/vendor/autoload.php
// Emits real installed Monolog JSON; no application is bootstrapped and no log file is written.
require $argv[1];
$formatter = new \Monolog\Formatter\JsonFormatter();
$formatter->includeStacktraces();
$stream = fopen('php://memory', 'w+');
$handler = new \Monolog\Handler\StreamHandler($stream);
$handler->setFormatter($formatter);
$logger = new \Monolog\Logger('fixture', [$handler]);
$logger->pushProcessor(static function ($record) {
    $record->extra['processor_field'] = 'PRIVATE_PROCESSOR_SENTINEL';
    return $record;
});
function fail_inner() { throw new \InvalidArgumentException('inner password=PRIVATE_PASSWORD_SENTINEL'); }
function fail_outer() {
    try { fail_inner(); }
    catch (\Throwable $previous) { throw new \RuntimeException('outer operation failed', 0, $previous); }
}
try { fail_outer(); }
catch (\Throwable $error) {
    $logger->error('request failed token=PRIVATE_TOKEN_SENTINEL', [
        'exception' => $error,
        'password' => 'PRIVATE_CONTEXT_SENTINEL',
        'request' => ['authorization' => 'PRIVATE_REQUEST_SENTINEL'],
    ]);
}
rewind($stream);
$record = json_decode(stream_get_contents($stream), true, 64, JSON_THROW_ON_ERROR);
$record['datetime'] = '2026-11-10T08:00:00+01:00';
$record['context']['exception']['file'] = str_replace(__FILE__, '/project/app/MonologFixture.php', $record['context']['exception']['file']);
$normalize = static function (&$exception) use (&$normalize) {
    if (isset($exception['file'])) { $exception['file'] = str_replace(__FILE__, '/project/app/MonologFixture.php', $exception['file']); }
    if (isset($exception['trace'])) { $exception['trace'] = array_map(static fn ($frame) => str_replace(__FILE__, '/project/app/MonologFixture.php', $frame), $exception['trace']); }
    if (isset($exception['previous'])) { $normalize($exception['previous']); }
};
$normalize($record['context']['exception']);
echo json_encode($record, JSON_THROW_ON_ERROR | JSON_UNESCAPED_SLASHES), "\n";
