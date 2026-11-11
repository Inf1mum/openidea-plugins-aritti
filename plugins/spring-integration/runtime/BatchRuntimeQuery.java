// Original reviewed-context execution metadata query. No framework source copied.
import java.nio.charset.StandardCharsets;
import java.util.HexFormat;

class BatchRuntimeQuery {
 static Object call(Object target,String method)throws Exception{return target.getClass().getMethod(method).invoke(target);}
 public static void main(String[]args)throws Exception {
  if(args.length!=4)throw new IllegalArgumentException("Expected exact profile, context, kind and execution ID");
  for(String key:new String[]{"JAVA_TOOL_OPTIONS","_JAVA_OPTIONS","JDK_JAVA_OPTIONS"})if(System.getenv(key)!=null&&!System.getenv(key).isBlank())throw new IllegalArgumentException("Inherited JVM injection options are outside this profile: "+key);
  String profile=args[0],contextName=args[1],kind=args[2];long id=Long.parseLong(args[3]);if(id<=0)throw new IllegalArgumentException("Positive execution ID required");
  boolean runner=profile.equals("runner-5.2.6");if(!runner&&!profile.equals("operator-6.0.6"))throw new IllegalArgumentException("Unsupported exact profile");
  Object context;
  if(kind.equals("java")){
   var type=Class.forName("org.springframework.context.annotation.AnnotationConfigApplicationContext");
   context=type.getConstructor(Class[].class).newInstance((Object)new Class<?>[]{Class.forName(contextName)});
  }else if(kind.equals("xml")&&runner){
   var type=Class.forName("org.springframework.context.support.ClassPathXmlApplicationContext");context=type.getConstructor(String[].class).newInstance((Object)new String[]{contextName});
  }else throw new IllegalArgumentException("Unsupported context kind");
  String result;
  try {
   Class<?> api=Class.forName(runner?"org.springframework.batch.core.explore.JobExplorer":"org.springframework.batch.core.repository.JobRepository");
   Object reader=context.getClass().getMethod("getBean",Class.class).invoke(context,api);
   Object execution=api.getMethod("getJobExecution",runner?Long.class:long.class).invoke(reader,id);
   var out=new StringBuilder("OPENIDEA_BATCH_EXECUTION_V1\nprofile="+profile+"\nexecution-id="+id+"\n");
   if(execution==null){out.append("found=false\n");}
   else {
    Object actual=call(execution,"getId");if(!(actual instanceof Number)||((Number)actual).longValue()!=id)throw new IllegalStateException("Repository returned a different execution identity");
    Object instance=call(execution,"getJobInstance");long instanceId=((Number)call(instance,"getInstanceId")).longValue();String jobName=(String)call(instance,"getJobName");Object status=call(execution,"getStatus");
    if(!(status instanceof Enum<?>))throw new IllegalStateException("Unsupported execution status type");
    Object start=call(execution,"getStartTime"),end=call(execution,"getEndTime");
    out.append("found=true\ninstance-id=").append(instanceId).append("\njob-name-hex=").append(HexFormat.of().formatHex(jobName.getBytes(StandardCharsets.UTF_8))).append("\nstatus=").append(((Enum<?>)status).name()).append("\nstart-time=").append(start==null?"-":start).append("\nend-time=").append(end==null?"-":end).append("\nobserved-at=").append(java.time.Instant.now()).append('\n');
   }
   result=out.toString();
  } finally {((AutoCloseable)context).close();}
  // Startup/shutdown logging precedes this final machine-readable record.
  System.out.print(result);
 }
}
