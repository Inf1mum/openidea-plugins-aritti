<?php
// Standalone disposable framework probe. Pass a trusted Laravel application's
// vendor/autoload.php. Requires its normal development Faker dependency.
// Uses an in-memory connection and never creates tables or persists models.
$autoload=$argv[1]??'';
if(!is_file($autoload)){fwrite(STDERR,"Usage: php semantic-runtime.php /trusted/project/vendor/autoload.php\n");exit(2);}
require $autoload;
use Illuminate\Database\Eloquent\Model;
use Illuminate\Database\Eloquent\Builder;
use Illuminate\Database\Eloquent\Collection;
use Illuminate\Database\Eloquent\Factories\Factory;
use Illuminate\Database\Eloquent\Factories\HasFactory;
use Illuminate\Database\Capsule\Manager;
use Illuminate\Database\Eloquent\Casts\Attribute;
$capsule=new Manager;
$capsule->addConnection(['driver'=>'sqlite','database'=>':memory:']);
$capsule->setAsGlobal();$capsule->bootEloquent();
Illuminate\Container\Container::setInstance($capsule->getContainer());
$capsule->getContainer()->singleton(Faker\Generator::class,fn()=>Faker\Factory::create());
class SemanticBuilder extends Builder {public function published():static{return $this;}}
class SemanticCollection extends Collection {public function reviewed():static{return $this;}}
class SemanticUserCollection extends Collection {}
class SemanticPost extends Model {
 use HasFactory;
 protected $table='posts';protected $dateFormat='Y-m-d H:i:s';
 protected $guarded=[];protected $casts=['options'=>'array'];
 public function newEloquentBuilder($query){return new SemanticBuilder($query);}
 public function newCollection(array $models=[]){return new SemanticCollection($models);}
 protected static function newFactory(){return SemanticFactory::new();}
 public function getTitleAttribute():?string{return null;}
}
class SemanticFactory extends Factory {protected $model=SemanticPost::class;public function definition():array{return ['title'=>'sample'];}public function reviewed():static{return $this;}}
class SemanticCar extends Model {public function owner(){return $this->hasOne(SemanticPost::class);}}
class SemanticMechanic extends Model {
 public function cars(){return $this->hasMany(SemanticCar::class);}
 public function car(){return $this->hasOne(SemanticCar::class);}
 public function owners(){return $this->through('cars')->has('owner');}
 public function owner(){return $this->throughCar()->hasOwner();}
}
$post=new SemanticPost;$post->setRawAttributes(['payload'=>'{}','options'=>'{}','published_at'=>'2026-01-01 00:00:00','created_at'=>'2026-01-01 00:00:00']);
class SemanticAttributes extends Model {
 protected $guarded=[];
 protected function label():Attribute{return Attribute::get(fn():?string=>null);}
 protected function secret():Attribute{return Attribute::set(fn(string $value)=>strtolower($value));}
 protected function stored():Attribute{return Attribute::get(fn(string $value):string=>strtoupper($value));}
 protected function related():Attribute{return Attribute::make(get:fn():SemanticPost=>new SemanticPost,set:fn(SemanticPost $value)=>$value->getAttributes());}
}
class SemanticClassicParent extends Model {public function getAliasAttribute():string{return 'classic';}}
class SemanticModernChild extends SemanticClassicParent {protected function alias():Attribute{return Attribute::get(fn():string=>'modern');}}
class SemanticUser extends Model {public function posts(){return $this->hasMany(SemanticPost::class);}public function newCollection(array $models=[]){return new SemanticUserCollection($models);}}
$attributes=new SemanticAttributes;$attributes->secret='VALUE';$attributes->stored='value';
$query=SemanticPost::query();
$eager=SemanticUser::with(['posts'=>function($query){if(!$query->getModel() instanceof SemanticPost){throw new RuntimeException('eager query model');}$query->published();}]);
($eager->getEagerLoads()['posts'])((new SemanticUser)->posts());
SemanticPost::resolveRelationUsing('arrowOne',static fn($post)=>$post->hasOne(SemanticPost::class));
SemanticPost::resolveRelationUsing('arrowMany',fn($post)=>$post->hasMany(SemanticPost::class));
$observed=[
 'rawJson'=>get_debug_type($post->payload),'castJson'=>get_debug_type($post->options),
 'rawDate'=>get_debug_type($post->published_at),'implicitTimestamp'=>get_debug_type($post->created_at),
 'accessor'=>get_debug_type($post->title),
 'modernRead'=>get_debug_type($attributes->label),'modernWrite'=>$attributes->secret,
 'storedGetterWrite'=>[$attributes->getAttributes()['stored'],$attributes->stored],
 'modernObject'=>get_class($attributes->related),'classicPrecedence'=>(new SemanticModernChild)->alias,
 'whenResult'=>get_class($query->when(true,fn($q)=>$q->getModel())),
 'unlessResult'=>get_class($query->unless(false,fn($q)=>$q->getModel())),
 'voidCondition'=>get_class($query->when(true,function($q){$q->published();})),
 'optional'=>get_class(optional($post)->newCollection()),
 'optionalCallback'=>get_class(optional($post,fn($model)=>$model)),
 'builder'=>get_class(SemanticPost::query()->published()),
 'collection'=>get_class((new SemanticPost)->newCollection()->reviewed()),
 'factoryOne'=>get_class(SemanticPost::factory()->reviewed()->make()),
 'factoryMany'=>get_class(SemanticPost::factory()->count(1)->make()),
 'factoryManyCount'=>SemanticPost::factory(3)->make()->count(),
 'mappedCollection'=>get_class(SemanticPost::factory(2)->make()->map(fn($model)=>$model)),
 'crossModelMap'=>get_class(SemanticPost::factory(2)->make()->map(fn($model)=>new SemanticUser)),
 'crossModelMapItem'=>get_class(SemanticPost::factory(2)->make()->map(fn($model)=>new SemanticUser)->reviewed()->first()),
 'crossModelFlatMap'=>get_class(SemanticPost::factory(2)->make()->flatMap(fn($model)=>new SemanticUserCollection([new SemanticUser]))),
 'flatMappedItem'=>get_class((new SemanticUser)->newCollection([new SemanticUser])->flatMap(fn($model)=>(new SemanticCar)->newCollection([new SemanticCar]))->first()),
 'arrowOne'=>get_class((new SemanticPost)->arrowOne()),'arrowMany'=>get_class((new SemanticPost)->arrowMany()),
 'newInstance'=>get_class($post->newInstance()),'newFromBuilder'=>get_class($post->newFromBuilder([])),'newModelInstance'=>get_class(SemanticPost::query()->newModelInstance()),
 'throughOne'=>get_class((new SemanticMechanic)->owner()),
 'throughMany'=>get_class((new SemanticMechanic)->owners()),
];
$expected=[
 'rawJson'=>'string','castJson'=>'array','rawDate'=>'string','implicitTimestamp'=>'Illuminate\\Support\\Carbon',
 'accessor'=>'null','modernRead'=>'null','modernWrite'=>'value','storedGetterWrite'=>['value','VALUE'],
 'modernObject'=>SemanticPost::class,'classicPrecedence'=>'classic',
 'whenResult'=>SemanticPost::class,'unlessResult'=>SemanticPost::class,'voidCondition'=>SemanticBuilder::class,
 'optional'=>SemanticCollection::class,'optionalCallback'=>SemanticPost::class,'builder'=>SemanticBuilder::class,'collection'=>SemanticCollection::class,
 'factoryOne'=>SemanticPost::class,'factoryMany'=>SemanticCollection::class,'factoryManyCount'=>3,
 'mappedCollection'=>SemanticCollection::class,'crossModelMap'=>SemanticCollection::class,'crossModelMapItem'=>SemanticUser::class,'crossModelFlatMap'=>'Illuminate\\Support\\Collection','flatMappedItem'=>SemanticCar::class,
 'arrowOne'=>'Illuminate\\Database\\Eloquent\\Relations\\HasOne','arrowMany'=>'Illuminate\\Database\\Eloquent\\Relations\\HasMany',
 'newInstance'=>SemanticPost::class,'newFromBuilder'=>SemanticPost::class,'newModelInstance'=>SemanticPost::class,
 'throughOne'=>'Illuminate\\Database\\Eloquent\\Relations\\HasOneThrough',
 'throughMany'=>'Illuminate\\Database\\Eloquent\\Relations\\HasManyThrough',
];
if($observed!==$expected){fwrite(STDERR,json_encode(['expected'=>$expected,'actual'=>$observed],JSON_PRETTY_PRINT));exit(1);}
echo json_encode($observed,JSON_PRETTY_PRINT),PHP_EOL;
