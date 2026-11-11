<?php
// PHP 8.5+ only. Standalone operator semantics probe; no application writes.
final class PipeValue
{
    public function __construct(public string $name = 'proven') {}
    public function fresh(): static { return $this; }
}
function keepPipeValue(PipeValue $value): PipeValue { return $value; }
final class PipeTransforms
{
    public static function keep(PipeValue $value): PipeValue { return $value; }
    public function keepInstance(PipeValue $value): PipeValue { return $value; }
}
$input = new PipeValue;
$result = $input
    |> (fn(PipeValue $value) => $value->fresh())
    |> keepPipeValue(...)
    |> PipeTransforms::keep(...)
    |> (new PipeTransforms)->keepInstance(...);
if ($result !== $input || $result->name !== 'proven') {
    throw new RuntimeException('Pipe input/result identity was not retained');
}
echo 'PASS PHP ', PHP_VERSION, ': typed arrow, first-class function/static/instance methods, chained object identity', PHP_EOL;
if (isset($argv[1])) {
    require $argv[1];
    $capsule = new Illuminate\Database\Capsule\Manager;
    $capsule->addConnection(['driver' => 'sqlite', 'database' => ':memory:']);
    $capsule->setAsGlobal();
    $capsule->bootEloquent();
    $capsule->schema()->create('pipe_users', function ($table) { $table->id(); $table->string('name'); });
    class FrameworkPipeUser extends Illuminate\Database\Eloquent\Model {
        protected $table = 'pipe_users';
        protected $guarded = [];
        public $timestamps = false;
    }
    function keepFrameworkPipeUser(FrameworkPipeUser $user): FrameworkPipeUser { return $user; }
    FrameworkPipeUser::create(['name' => 'proven']);
    $result = FrameworkPipeUser::first()
        |> (fn(FrameworkPipeUser $user) => $user->fresh())
        |> keepFrameworkPipeUser(...);
    if (!$result instanceof FrameworkPipeUser || $result->name !== 'proven') {
        throw new RuntimeException('Framework pipe did not retain the proven model result');
    }
    echo 'PASS Laravel model pipe: first, typed arrow, fresh, first-class function; SQLite memory only', PHP_EOL;
}
