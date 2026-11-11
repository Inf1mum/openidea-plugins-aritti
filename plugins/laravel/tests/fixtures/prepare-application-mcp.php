<?php
// Authored acceptance fixture only. Never invoke against a user's application.
$root = realpath($argv[1] ?? '');
if ($root === false || !str_starts_with(basename($root), 'laravel-mcp-milestone6-') || !is_file($root.'/vendor/laravel/mcp/composer.json')) { throw new RuntimeException('Select a disposable laravel-mcp-milestone6-* app with laravel/mcp installed.'); }
$directory = $root.'/app/Mcp/Fixture';
if (!is_dir($directory)) { mkdir($directory, 0777, true); }
$files = [
'GuardTool.php' => <<<'PHP'
<?php
namespace App\Mcp\Fixture;
use Illuminate\Contracts\JsonSchema\JsonSchema;
class GuardTool extends \Laravel\Mcp\Server\Tool {
    protected string $name = 'milestone-weather';
    protected string $description = 'Authored metadata acceptance tool; its handle must never execute during discovery.';
    public function schema(JsonSchema $schema): array { return ['city' => $schema->string()->required()]; }
    public function handle(\Laravel\Mcp\Request $request): \Laravel\Mcp\Response { file_put_contents(base_path('mcp-handler-executed.marker'), 'tool'); return \Laravel\Mcp\Response::text('not expected'); }
}
PHP,
'GuardResource.php' => <<<'PHP'
<?php
namespace App\Mcp\Fixture;
class GuardResource extends \Laravel\Mcp\Server\Resource {
    protected string $name = 'milestone-guide';
    protected string $uri = 'fixture://guide';
    public function handle(\Laravel\Mcp\Request $request): \Laravel\Mcp\Response { file_put_contents(base_path('mcp-handler-executed.marker'), 'resource'); return \Laravel\Mcp\Response::text('not expected'); }
}
PHP,
'GuardPrompt.php' => <<<'PHP'
<?php
namespace App\Mcp\Fixture;
class GuardPrompt extends \Laravel\Mcp\Server\Prompt {
    protected string $name = 'milestone-summary';
    public function arguments(): array { return [new \Laravel\Mcp\Server\Prompts\Argument('city', 'City name', true)]; }
    public function handle(\Laravel\Mcp\Request $request): \Laravel\Mcp\Response { file_put_contents(base_path('mcp-handler-executed.marker'), 'prompt'); return \Laravel\Mcp\Response::text('not expected'); }
}
PHP,
'GuardServer.php' => <<<'PHP'
<?php
namespace App\Mcp\Fixture;
class GuardServer extends \Laravel\Mcp\Server {
    protected string $name = 'Milestone MCP fixture';
    protected array $tools = [GuardTool::class];
    protected array $resources = [GuardResource::class];
    protected array $prompts = [GuardPrompt::class];
}
PHP,
];
foreach ($files as $file => $text) { file_put_contents($directory.'/'.$file, $text); }
file_put_contents($root.'/routes/ai.php', <<<'PHP'
<?php
use Laravel\Mcp\Facades\Mcp;
use App\Mcp\Fixture\GuardServer;
Mcp::local('milestone', GuardServer::class);
Mcp::web('/mcp/milestone', GuardServer::class);
PHP);
file_put_contents($root.'/mcp-milestone6.fixture', 'openidea-laravel-mcp-fixture-v1');
echo "Prepared authored application MCP fixtures.\n";
