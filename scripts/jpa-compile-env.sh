# Source me: exports the env vars used by the opt-in generated_* compile tests.
# Missing jars warn on stderr and leave the variable unset.
_jpa_find() {
    _jpa_found=
    for _jpa_candidate in "$@"; do
        if [ -f "$_jpa_candidate" ]; then
            _jpa_found=$_jpa_candidate
            return 0
        fi
    done
    return 1
}
_jpa_export() {
    _jpa_name=$1
    shift
    if _jpa_find "$@"; then
        export "$_jpa_name=$_jpa_found"
    else
        echo "jpa-compile-env: $_jpa_name skipped, no jar found" >&2
    fi
}

_jpa_jdk=/Library/Java/JavaVirtualMachines/temurin-25.jdk/Contents/Home/bin
if [ -d "$_jpa_jdk" ]; then
    export PATH="$_jpa_jdk:$PATH"
else
    echo "jpa-compile-env: JDK not found at $_jpa_jdk" >&2
fi

_jpa_api=/tmp/jpa-parity-compile-api
_jpa_m2=$HOME/.m2/repository
_jpa_export JPA_JAKARTA_API "$_jpa_api/jakarta.persistence-api-3.2.0.jar"
_jpa_export JPA_JAVAX_API "$_jpa_api/javax.persistence-api-2.2.jar"
_jpa_export KOTLIN_STDLIB "$_jpa_m2/org/jetbrains/kotlin/kotlin-stdlib/2.2.21/kotlin-stdlib-2.2.21.jar"
_jpa_export MAPSTRUCT_JAR "$_jpa_m2/org/mapstruct/mapstruct/1.6.3/mapstruct-1.6.3.jar"
_jpa_export MAPSTRUCT_PROCESSOR_JAR "$_jpa_m2/org/mapstruct/mapstruct-processor/1.6.3/mapstruct-processor-1.6.3.jar"

if [ -f "$_jpa_api/compiler-classpath.txt" ]; then
    KOTLIN_COMPILER_CLASSPATH=$(cat "$_jpa_api/compiler-classpath.txt")
    export KOTLIN_COMPILER_CLASSPATH
else
    echo "jpa-compile-env: KOTLIN_COMPILER_CLASSPATH skipped, $_jpa_api/compiler-classpath.txt missing" >&2
fi

_jpa_spring=
_jpa_missing=
for _jpa_jar in \
    "$_jpa_m2/org/springframework/data/spring-data-commons/3.4.13/spring-data-commons-3.4.13.jar" \
    "$_jpa_m2/org/springframework/data/spring-data-jpa/4.1.2/spring-data-jpa-4.1.2.jar" \
    "$_jpa_m2/org/springframework/spring-core/6.2.19/spring-core-6.2.19.jar" \
    "$_jpa_m2/org/slf4j/slf4j-api/2.0.17/slf4j-api-2.0.17.jar" \
    "$_jpa_api/jakarta.persistence-api-3.2.0.jar"; do
    if [ -f "$_jpa_jar" ]; then
        _jpa_spring=${_jpa_spring:+$_jpa_spring:}$_jpa_jar
    else
        _jpa_missing="$_jpa_missing $_jpa_jar"
    fi
done
if [ -n "$_jpa_spring" ]; then
    export SPRING_DATA_CLASSPATH=$_jpa_spring
fi
if [ -n "$_jpa_missing" ]; then
    echo "jpa-compile-env: SPRING_DATA_CLASSPATH missing:$_jpa_missing" >&2
fi

_jpa_ctx=
for _jpa_jar in \
    "$_jpa_m2/org/springframework/spring-context/6.2.19/spring-context-6.2.19.jar" \
    "$_jpa_m2/org/springframework/spring-aop/6.2.19/spring-aop-6.2.19.jar" \
    "$_jpa_m2/org/springframework/spring-beans/6.2.19/spring-beans-6.2.19.jar" \
    "$_jpa_m2/org/springframework/spring-core/6.2.19/spring-core-6.2.19.jar"; do
    [ -f "$_jpa_jar" ] && _jpa_ctx=${_jpa_ctx:+$_jpa_ctx:}$_jpa_jar
done
[ -n "$_jpa_ctx" ] && export SPRING_CONTEXT_CLASSPATH=$_jpa_ctx

_jpa_jb=$HOME/Library/Application\ Support/JetBrains
_jpa_javax_validation=$(find "$_jpa_jb" -name 'validation-api-2.0.1.Final.jar' 2>/dev/null | head -n 1)
_jpa_jakarta_validation=$(find "$_jpa_m2" "$_jpa_jb" -name 'jakarta.validation-api-*.jar' ! -name '*sources*' ! -name '*javadoc*' 2>/dev/null | head -n 1)
_jpa_export JPA_JAVAX_VALIDATION_API "$_jpa_javax_validation"
_jpa_export JPA_JAKARTA_VALIDATION_API "$_jpa_jakarta_validation"

_jpa_hib=$(find "$_jpa_m2/org/hibernate/orm/hibernate-core/6.6.49.Final" -name 'hibernate-core-6.6.49.Final.jar' 2>/dev/null | head -n 1)
_jpa_export HIBERNATE_CORE_JAR "$_jpa_hib"
unset _jpa_hib

_jpa_fw=$(find "$_jpa_api" "$_jpa_m2/org/flywaydb/flyway-core" -name 'flyway-core-10*.jar' ! -name '*sources*' 2>/dev/null | head -n 1)
_jpa_export FLYWAY_CORE_JAR "$_jpa_fw"
unset _jpa_fw

_jpa_envers=
for _jpa_jar in \
    "$_jpa_api/spring-data-envers-4.1.2.jar" \
    "$_jpa_m2/org/springframework/data/spring-data-commons/4.1.2/spring-data-commons-4.1.2.jar" \
    "$_jpa_m2/org/springframework/data/spring-data-jpa/4.1.2/spring-data-jpa-4.1.2.jar" \
    "$_jpa_m2/org/springframework/spring-core/7.0.10/spring-core-7.0.10.jar" \
    "$_jpa_m2/org/springframework/spring-context/7.0.10/spring-context-7.0.10.jar" \
    "$_jpa_m2/org/springframework/spring-beans/7.0.10/spring-beans-7.0.10.jar" \
    "$_jpa_api/jakarta.persistence-api-3.2.0.jar"; do
    [ -f "$_jpa_jar" ] && _jpa_envers=${_jpa_envers:+$_jpa_envers:}$_jpa_jar
done
[ -n "$_jpa_envers" ] && export SPRING_DATA_ENVERS_CLASSPATH=$_jpa_envers
_jpa_blaze=
for _jpa_jar in \
    "$_jpa_api/blaze-persistence-integration-spring-data-base-3.3-1.6.20.jar" \
    "$_jpa_m2/org/springframework/data/spring-data-commons/4.1.2/spring-data-commons-4.1.2.jar"; do
    [ -f "$_jpa_jar" ] && _jpa_blaze=${_jpa_blaze:+$_jpa_blaze:}$_jpa_jar
done
[ -n "$_jpa_blaze" ] && export BLAZE_SPRING_DATA_CLASSPATH=$_jpa_blaze
unset _jpa_envers _jpa_blaze

unset -f _jpa_find _jpa_export
unset _jpa_found _jpa_candidate _jpa_name _jpa_jdk _jpa_api _jpa_m2 _jpa_spring _jpa_missing _jpa_jar _jpa_jb _jpa_javax_validation _jpa_jakarta_validation
