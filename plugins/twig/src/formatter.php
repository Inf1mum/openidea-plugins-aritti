// Pinned official Twig-CS-Fixer in-memory adapter. No project configuration or files are loaded.
error_reporting(0);
try {
    $file = realpath($argv[1]);
    if (!$file || !is_file($file) || !str_ends_with($file, '.phar') || !hash_equals($argv[2], hash_file('sha256', $file))) { exit(2); }
    $input = stream_get_contents(STDIN, 1048577);
    if (strlen($input) > 1048576) { exit(3); }
    $request = json_decode($input, true, 8, JSON_THROW_ON_ERROR);
    $source = $request['source'] ?? null;
    if (!is_string($source) || strlen($source) > 131072) { exit(3); }
    require 'phar://'.$file.'/vendor/autoload.php';
    $env = new TwigCsFixer\Environment\StubbedEnvironment();
    $tokenizer = new TwigCsFixer\Token\Tokenizer($env);
    $fixer = new TwigCsFixer\Runner\Fixer($tokenizer);
    $rules = new TwigCsFixer\Ruleset\Ruleset();
    foreach ((new TwigCsFixer\Standard\Twig())->getRules() as $rule) {
        if (str_ends_with($rule::class, 'SpacingRule')) { $rules->addRule($rule); }
    }
    $result = $fixer->fixFile($source, $rules);
    if (strlen($result) > 262144 || !hash_equals($argv[2], hash_file('sha256', $file))) { exit(3); }
    echo json_encode(['version'=>1, 'source'=>$source, 'result'=>$result], JSON_THROW_ON_ERROR);
} catch (Throwable $e) { exit(4); }
