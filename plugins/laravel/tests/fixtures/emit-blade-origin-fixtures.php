<?php
// Authored acceptance only; executes erroring views in a disposable app.
$root = realpath($argv[1] ?? '');
if ($root === false || !str_starts_with(basename($root), 'laravel-mcp-milestone6-') || !is_file($root.'/mcp-milestone6.fixture')) { throw new RuntimeException('Select the guarded disposable Laravel MCP acceptance app.'); }
chdir($root);
require $root.'/vendor/autoload.php';
$app = require $root.'/bootstrap/app.php';
$app->make(\Illuminate\Contracts\Console\Kernel::class)->bootstrap();
$directory=$root.'/resources/views';
file_put_contents($directory.'/origin-child.blade.php', "Child\n{{ missing_origin_child() }}");
$cases = [
 ['echo', "Heading\n<p>{{ missing_origin_echo() }}</p>",1],
 ['if', "Heading\n@if (missing_origin_if())\nbody\n@endif",1],
 ['foreach', "Heading\n@foreach (missing_origin_items() as \$item)\n{{ \$item }}\n@endforeach",1],
 ['php', "Heading\n@php\nthrow new \\RuntimeException('origin-php');\n@endphp",2],
 ['include', "Heading\n@include('origin-child', ['id'=>123])",1],
 ['includeWhen', "Heading\n@includeWhen(missing_origin_when(), 'origin-child')",1],
 ['ambiguous', "Heading\n@if(false)\n{{ missing_origin_repeat() }}\n@endif\n{{ missing_origin_repeat() }}",null],
];
$output=[];
foreach($cases as [$name,$source,$expected]) {
 $view='origin-fixture-'.$name;$path=$directory.'/'.$view.'.blade.php';file_put_contents($path,$source);
 try {$app['view']->make($view)->render();throw new RuntimeException('Expected authored view error.');}
 catch (\Throwable $error) {
  $frames=[];$seen=[];for($current=$error;$current!==null;$current=$current->getPrevious()){
   $locations=array_merge([['file'=>$current->getFile(),'line'=>$current->getLine()]],$current->getTrace());
   foreach($locations as $frame){$file=$frame['file']??'';$line=$frame['line']??0;if(!str_starts_with($file,$root.'/storage/framework/views/')||!is_file($file)||isset($seen[$file.':'.$line])){continue;}$seen[$file.':'.$line]=true;
    $compiled=file_get_contents($file);if(!preg_match('~/\*\*PATH (.*?) ENDPATH\*\*/~s',$compiled,$match)){continue;}$origin=$match[1];if(!str_starts_with($origin,$directory.'/')||!is_file($origin)){continue;}
    $frames[]=['compiledPath'=>$file,'compiled'=>$compiled,'line'=>$line,'originalPath'=>$origin,'source'=>file_get_contents($origin),'expectedLine'=>$origin===$path?$expected:1];
   }
  }
  if($frames===[]){throw new RuntimeException('No real compiled view frame for '.$name);}$output[]=['name'=>$name,'frames'=>$frames];
 }
}
echo json_encode(['root'=>$root,'cases'=>$output],JSON_THROW_ON_ERROR);
