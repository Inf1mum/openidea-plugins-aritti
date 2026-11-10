<?php
namespace {
// Run after exporting generated fixtures from bulk tests.
$autoload=$argv[1] ?? throw new \RuntimeException('Pass the installed Laravel vendor/autoload.php');
$fixtures=$argv[2] ?? throw new \RuntimeException('Pass the exported related-generation fixture directory');
require $autoload;
$capsule = new \Illuminate\Database\Capsule\Manager;
$capsule->addConnection(['driver'=>'sqlite','database'=>':memory:']);
$capsule->setAsGlobal(); $capsule->bootEloquent();
\Illuminate\Container\Container::getInstance()->instance(\Faker\Generator::class, \Faker\Factory::create());
$capsule->schema()->create('users', function ($t) {$t->id();$t->string('name');});
$capsule->schema()->create('posts', function ($t) {$t->id();$t->string('title');$t->foreignId('external_id')->constrained('users');});
}
namespace Domain\Models {
class User extends \Illuminate\Database\Eloquent\Model { public $timestamps=false; protected $guarded=[]; }
class Post extends \Illuminate\Database\Eloquent\Model {
public $timestamps=false; protected $guarded=[]; protected $casts=['cast_only'=>'integer'];
public function getDisplayNameAttribute():string{return 'Visible';}
public function getLabelAttribute(){return 'Label';}
}
}
namespace Domain\Factories {
class OwnerFactory extends \Illuminate\Database\Eloquent\Factories\Factory {
protected $model=\Domain\Models\User::class;
public function definition():array{return ['name'=>'Factory owner'];}
}
}
namespace Domain\Support {
abstract class BaseFactory extends \Illuminate\Database\Eloquent\Factories\Factory {}
class BaseResource extends \Illuminate\Http\Resources\Json\JsonResource {}
}
namespace Domain\Resources {
class PersonResource extends \Illuminate\Http\Resources\Json\JsonResource {}
}
namespace {
require $fixtures.'/selected_factory.php';
require $fixtures.'/selected_resource.php';
require $fixtures.'/configured_factory.php';
require $fixtures.'/configured_resource.php';
require $fixtures.'/rich_resource.php';
require $fixtures.'/runtime_resource.php';
if (!is_subclass_of(\Domain\Generated\Factories\Custom::class, \Domain\Support\BaseFactory::class)
    || !is_subclass_of(\Domain\Generated\Resources\Custom::class, \Domain\Support\BaseResource::class)) {
    throw new \RuntimeException('Configured related base classes were not used');
}
foreach ([[\Domain\Generated\Factories\Custom::class,'definition'],[\Domain\Generated\Resources\Custom::class,'toArray']] as [$class,$method]) {
    if ((new \ReflectionMethod($class,$method))->hasReturnType()) {throw new \RuntimeException('Disabled optional return hints were emitted');}
}

$post=\Database\Factories\PostFactory::new()->create();
if (!$post->exists || \Domain\Models\User::find($post->external_id)?->name !== 'Factory owner') {throw new \RuntimeException('Foreign factory mapping did not persist');}
$request=\Illuminate\Http\Request::create('/');
$resource=new \Domain\Http\Resources\PostResource($post);
$data=$resource->resolve($request);
if (array_keys($data)!==['title']) {throw new \RuntimeException('Unloaded relations were included');}
$user=\Domain\Models\User::findOrFail($post->external_id);
$post->setRelation('author',$user);$post->setRelation('readers',new \Illuminate\Database\Eloquent\Collection([$user]));
$data=$resource->resolve($request);
if($data['author']['name']!=='Factory owner'||$data['readers'][0]['name']!=='Factory owner'){throw new \RuntimeException('Resource cardinality/wrapping failed');}
$post->cast_only='7';$post->phpdoc_only='Documented';$post->runtime_only='Captured';
$rich=(new \Domain\Http\Resources\RichResource($post))->resolve($request);
if ($rich!==['cast_only'=>7,'display_name'=>'Visible','label'=>'Label','phpdoc_only'=>'Documented']) {throw new \RuntimeException('Rich source field values were not preserved');}
$runtime=(new \Domain\Http\Resources\RuntimeResource($post))->resolve($request);
if ($runtime!==['runtime_only'=>'Captured']) {throw new \RuntimeException('Captured field resource output failed');}
echo "PASS real Laravel: selected factory persists foreign key; resource omits unloaded relations and wraps one/many loaded relations; configured bases and optional type hints load correctly; rich and captured fields resolve correctly\n";
}
