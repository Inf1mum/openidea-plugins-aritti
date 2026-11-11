<?php
// Native Laravel 11 framework contract fixture; see LICENSE.
class FactoryContract
{
 public static $namespace = 'Database\\Factories\\';
public function modelName()
    {
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
