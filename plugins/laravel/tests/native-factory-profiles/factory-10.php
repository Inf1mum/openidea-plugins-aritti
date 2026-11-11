<?php
// Native Laravel 10 framework contract fixture; see LICENSE.
class FactoryContract
{
 public static $namespace = 'Database\\Factories\\';
public function modelName()
    {
        $resolver = static::$modelNameResolver ?? function (self $factory) {
            $namespacedFactoryBasename = Str::replaceLast(
                'Factory', '', Str::replaceFirst(static::$namespace, '', get_class($factory))
            );

            $factoryBasename = Str::replaceLast('Factory', '', class_basename($factory));

            $appNamespace = static::appNamespace();

            return class_exists($appNamespace.'Models\\'.$namespacedFactoryBasename)
                        ? $appNamespace.'Models\\'.$namespacedFactoryBasename
                        : $appNamespace.$factoryBasename;
        };

        return $this->model ?? $resolver($this);
    }
protected static function appNamespace()
    {
        try {
            return Container::getInstance()
                            ->make(Application::class)
                            ->getNamespace();
        } catch (Throwable $e) {
            return 'App\\';
        }
    }
}
