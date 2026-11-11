<?php
// Native Laravel 13 framework contract fixture; see LICENSE.
class FactoryContract
{
 public static $namespace = 'Database\\Factories\\';
public function modelName()
    {
        if (! array_key_exists(static::class, static::$cachedModelAttributes)) {
            $attribute = (new ReflectionClass($this))->getAttributes(UseModel::class);

            static::$cachedModelAttributes[static::class] = $attribute !== []
                ? $attribute[0]->newInstance()->class
                : false;
        }

        if (static::$cachedModelAttributes[static::class]) {
            return static::$cachedModelAttributes[static::class];
        }

        if ($this->model !== null) {
            return $this->model;
        }

        $resolver = static::$modelNameResolvers[static::class] ?? static::$modelNameResolvers[self::class] ?? static::$modelNameResolver ?? function (self $factory) {
            $namespacedFactoryBasename = Str::replaceLast(
                'Factory', '', Str::replaceFirst(static::$namespace, '', $factory::class)
            );

            $factoryBasename = Str::replaceLast('Factory', '', class_basename($factory));

            $appNamespace = static::appNamespace();

            return class_exists($appNamespace.'Models\\'.$namespacedFactoryBasename)
                ? $appNamespace.'Models\\'.$namespacedFactoryBasename
                : $appNamespace.$factoryBasename;
        };

        return $resolver($this);
    }
protected static function appNamespace()
    {
        try {
            return Container::getInstance()
                ->make(Application::class)
                ->getNamespace();
        } catch (Throwable) {
            return 'App\\';
        }
    }
}
