<?php
// Disposable installed-package differential. SQLite memory only; no project writes.
$autoload=$argv[1]??'';
if(!is_file($autoload)){fwrite(STDERR,"Pass trusted fixture vendor/autoload.php\n");exit(2);}
require $autoload;
use Illuminate\Database\Capsule\Manager;
use Illuminate\Database\Eloquent\Model;
use Illuminate\Database\Schema\Blueprint;
use Kirschbaum\PowerJoins\EloquentJoins;
use Staudenmeir\LaravelCte\Eloquent\QueriesExpressions;
class PackageQueryCapsule extends Manager{
 protected function setupManager(){
  (new Staudenmeir\LaravelCte\DatabaseServiceProvider($this->container))->register();
  $this->manager=new Illuminate\Database\DatabaseManager($this->container,$this->container->make('db.factory'));
 }
}
$capsule=new PackageQueryCapsule;$capsule->addConnection(['driver'=>'sqlite','database'=>':memory:']);$capsule->setAsGlobal();$capsule->bootEloquent();
EloquentJoins::registerEloquentMacros();
class PackageQueryUser extends Model{protected $table='users';public $timestamps=false;}
class PackageQueryPost extends Model{
 use QueriesExpressions;protected $table='posts';public $timestamps=false;
 public function author(){return $this->belongsTo(PackageQueryUser::class,'user_id');}
}
$capsule->schema()->create('users',function(Blueprint $t){$t->id();$t->string('email');$t->unsignedBigInteger('parent_id')->nullable();});
$capsule->schema()->create('posts',function(Blueprint $t){$t->id();$t->unsignedBigInteger('user_id');$t->string('title');});
$capsule->table('users')->insert([['id'=>1,'email'=>'fixture@example.test','parent_id'=>null],['id'=>2,'email'=>'child@example.test','parent_id'=>1]]);$capsule->table('posts')->insert(['id'=>1,'user_id'=>1,'title'=>'Fixture']);
$callbackClass=null;
$joined=PackageQueryPost::joinRelationship('author',function($join)use(&$callbackClass){$callbackClass=get_class($join);$join->where('users.email','fixture@example.test');});
$cte=PackageQueryPost::withExpression('visible_posts',PackageQueryPost::select('posts.*'))->from('visible_posts as posts');
$treeQuery=PackageQueryUser::whereNull('parent_id')->unionAll(PackageQueryUser::select('users.*')->join('tree','tree.id','=','users.parent_id'));
$tree=PackageQueryUser::from('tree')->withRecursiveExpression('tree',$treeQuery)->get();
$observed=[
 'database'=>Composer\InstalledVersions::getPrettyVersion('illuminate/database'),
 'cte'=>Composer\InstalledVersions::getPrettyVersion('staudenmeir/laravel-cte'),
 'powerJoins'=>Composer\InstalledVersions::getPrettyVersion('kirschbaum-development/eloquent-power-joins'),
 'cteBuilder'=>get_class($cte->getQuery()),'cteModel'=>get_class($cte->firstOrFail()),
 'recursiveModel'=>get_class($tree->first()),'recursiveCount'=>$tree->count(),
 'providerOnlyBuilder'=>get_class(PackageQueryUser::withExpression('visible_users',PackageQueryUser::select('users.*'))->getQuery()),
 'providerOnlyModel'=>get_class(PackageQueryUser::withExpression('visible_users',PackageQueryUser::select('users.*'))->firstOrFail()),
 'joinedModel'=>get_class($joined->firstOrFail()),'leftCollection'=>get_class(PackageQueryPost::leftJoinRelationship('author')->get()),
 'leftModel'=>get_class(PackageQueryPost::leftJoinRelationship('author')->get()->first()),'callback'=>$callbackClass,
];
if($observed['recursiveModel']!==PackageQueryUser::class||$observed['recursiveCount']!==2){throw new RuntimeException('recursive model cardinality');}
foreach(['cteModel','joinedModel','leftModel']as$key){if($observed[$key]!==PackageQueryPost::class){throw new RuntimeException($key);}}
if($observed['providerOnlyBuilder']!=='Staudenmeir\\LaravelCte\\Query\\Builder'||$observed['providerOnlyModel']!==PackageQueryUser::class){throw new RuntimeException('provider-only Eloquent CTE');}
if($observed['cteBuilder']!=='Staudenmeir\\LaravelCte\\Query\\Builder'||$observed['callback']!=='Kirschbaum\\PowerJoins\\PowerJoinClause'){throw new RuntimeException('builder identity');}
echo json_encode($observed,JSON_PRETTY_PRINT),PHP_EOL;
