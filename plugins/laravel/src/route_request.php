<?php
// Explicit "Send Request": runs one route through the app's HTTP kernel inside this process.
// No server and no network. With rollback on, nothing is sent unless a transaction really started.
$marker = "\n@@laravel-request@@";
$reply = static function (array $payload) use ($marker): void {
    fwrite(STDOUT, $marker.json_encode($payload, JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE | JSON_INVALID_UTF8_SUBSTITUTE | JSON_PARTIAL_OUTPUT_ON_ERROR)."\n");
};
$notSent = static function (string $reason) use ($reply): void {
    $reply(['notSent' => true, 'error' => $reason]);
    exit(0);
};
$input = json_decode($argv[1] ?? '', true);
if (!is_array($input)) { $notSent('Nothing was sent: the request was not understood.'); }
error_reporting(E_ALL & ~E_DEPRECATED & ~E_USER_DEPRECATED);

$method = strtoupper((string) ($input['method'] ?? ''));
$uri = (string) ($input['uri'] ?? '');
$query = ltrim((string) ($input['query'] ?? ''), '?');
$body = $input['body'] ?? null;
$json = (bool) ($input['json'] ?? true);
$rollback = ($input['rollback'] ?? true) !== false;
$userId = $input['user'] ?? null;
if (!in_array($method, ['GET', 'HEAD', 'POST', 'PUT', 'PATCH', 'DELETE', 'OPTIONS'], true) || $uri === '' || $uri[0] !== '/' || strlen($uri) > 2048 || strlen($query) > 4096 || preg_match('/[\x00-\x20]/', $uri)) {
    $notSent('Nothing was sent: the method or URI is not valid.');
}

try {
    require getcwd().'/vendor/autoload.php';
    $app = require getcwd().'/bootstrap/app.php';
    $kernel = $app->make(Illuminate\Contracts\Http\Kernel::class);
    $kernel->bootstrap();
} catch (Throwable $e) {
    $notSent('Nothing was sent: the app did not boot ('.substr($e->getMessage(), 0, 300).').');
}

$connection = null;
if ($rollback) {
    try {
        $connection = $app['db']->connection();
        $connection->beginTransaction();
        if ($connection->transactionLevel() < 1) {
            throw new RuntimeException('the connection reports no open transaction');
        }
    } catch (Throwable $e) {
        try {
            while ($connection !== null && $connection->transactionLevel() > 0) { $connection->rollBack(); }
        } catch (Throwable $ignored) {
        }
        $notSent('Nothing was sent: Roll back database changes is on, but no transaction could be started on the default connection ('.substr($e->getMessage(), 0, 300).'). Turn Roll back database changes off to send the request for real.');
    }
}

// Counting is separate from the transaction: failing to count must never turn rollback off.
$queries = null;
try {
    ($connection ?? $app['db']->connection())->listen(static function () use (&$queries): void { $queries++; });
    $queries = 0;
} catch (Throwable $e) {
    $queries = null;
}

// Reports whether the transaction was still open to roll back; false when the app ended it itself.
$finish = static function () use (&$connection): ?bool {
    if ($connection === null) { return null; }
    try {
        $open = $connection->transactionLevel() > 0;
        while ($connection->transactionLevel() > 0) { $connection->rollBack(); }
        return $open;
    } catch (Throwable $e) {
        return false;
    }
};

try {
    $passThrough = new class { public function handle($request, $next) { return $next($request); } };
    foreach ([
        'Illuminate\Foundation\Http\Middleware\VerifyCsrfToken',
        'Illuminate\Foundation\Http\Middleware\ValidateCsrfToken',
        'Illuminate\Foundation\Http\Middleware\PreventRequestForgery',
        'App\Http\Middleware\VerifyCsrfToken',
    ] as $csrf) {
        $app->instance($csrf, $passThrough);
    }

    $server = ['HTTP_ACCEPT' => $json ? 'application/json' : 'text/html,application/xhtml+xml', 'REMOTE_ADDR' => '127.0.0.1', 'HTTP_USER_AGENT' => 'Laravel OpenIDEA request'];
    $content = null;
    $parameters = [];
    if (is_array($body) && !in_array($method, ['GET', 'HEAD'], true)) {
        if ($json) {
            $server['CONTENT_TYPE'] = 'application/json';
            $content = json_encode($body);
        } else {
            $parameters = $body;
        }
    }
    $request = Illuminate\Http\Request::create($uri.($query !== '' ? '?'.$query : ''), $method, $parameters, [], [], $server, $content);
    $app->instance('request', $request);

    if ($userId !== null && $userId !== '') {
        $model = config('auth.providers.users.model');
        $user = is_string($model) && class_exists($model) ? $model::find($userId) : null;
        if ($user === null) {
            $finish();
            $notSent('Nothing was sent: no user with id '.$userId.' in '.(is_string($model) ? $model : 'the users provider').'.');
        }
        $app['auth']->guard()->setUser($user);
    }

    $started = microtime(true);
    $response = $kernel->handle($request);
    $ms = round((microtime(true) - $started) * 1000, 1);

    if ($response instanceof Symfony\Component\HttpFoundation\BinaryFileResponse) {
        $text = '(file '.$response->getFile()->getFilename().', '.$response->getFile()->getSize().' bytes)';
    } elseif ($response instanceof Symfony\Component\HttpFoundation\StreamedResponse) {
        ob_start();
        $response->sendContent();
        $text = (string) ob_get_clean();
    } else {
        $text = (string) $response->getContent();
    }
    $size = strlen($text);
    if ($size > 200000) { $text = substr($text, 0, 200000); }

    // Cookies and credential headers are never returned, only the cookie names.
    $masked = ['set-cookie', 'cookie', 'authorization', 'proxy-authorization', 'x-csrf-token', 'x-xsrf-token'];
    $headers = [];
    foreach (array_slice($response->headers->all(), 0, 64, true) as $name => $values) {
        if (in_array(strtolower($name), $masked, true)) { continue; }
        $headers[$name] = substr(implode(', ', $values), 0, 2000);
    }
    $cookies = [];
    foreach (array_slice($response->headers->getCookies(), 0, 32) as $cookie) {
        $cookies[] = $cookie->getName().'=•••';
    }
    if ($cookies !== []) { $headers['set-cookie'] = implode(', ', $cookies); }

    $kernel->terminate($request, $response);
    $rolledBack = $finish();

    $reply([
        'status' => $response->getStatusCode(),
        'reason' => Symfony\Component\HttpFoundation\Response::$statusTexts[$response->getStatusCode()] ?? '',
        'headers' => $headers,
        'body' => $text,
        'size' => $size,
        'ms' => $ms,
        'queries' => $queries,
        'redirect' => $response->headers->get('Location'),
        'rollbackRequested' => $rollback,
        'rolledBack' => $rolledBack,
    ]);
} catch (Throwable $e) {
    $rolledBack = $finish();
    $reply(['error' => get_class($e).': '.substr($e->getMessage(), 0, 500), 'rollbackRequested' => $rollback, 'rolledBack' => $rolledBack]);
}
