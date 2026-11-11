package fixture;
import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Configuration;
import org.springframework.batch.core.Job;
import org.springframework.batch.core.job.builder.JobBuilder;
import org.springframework.batch.core.step.builder.StepBuilder;
import org.springframework.batch.core.repository.JobRepository;
import org.springframework.batch.core.repository.support.JobRepositoryFactoryBean;
import org.springframework.batch.core.explore.JobExplorer;
import org.springframework.batch.core.explore.support.JobExplorerFactoryBean;
import org.springframework.batch.core.launch.support.TaskExecutorJobLauncher;
import org.springframework.batch.core.configuration.JobRegistry;
import org.springframework.batch.core.configuration.support.MapJobRegistry;
import org.springframework.batch.core.configuration.support.ReferenceJobFactory;
import org.springframework.batch.repeat.RepeatStatus;
import org.springframework.jdbc.datasource.DriverManagerDataSource;
import org.springframework.jdbc.datasource.DataSourceTransactionManager;
import org.springframework.jdbc.datasource.init.ResourceDatabasePopulator;
import org.springframework.core.io.ClassPathResource;
import org.springframework.transaction.PlatformTransactionManager;
import javax.sql.DataSource;
@Configuration
public class BatchContext {
 @Bean public DataSource dataSource() {
  var source=new DriverManagerDataSource("jdbc:h2:file:./state/batch;AUTO_SERVER=TRUE","sa","");
  var schema=new ResourceDatabasePopulator(new ClassPathResource("org/springframework/batch/core/schema-h2.sql"));schema.setContinueOnError(true);schema.execute(source);return source;
 }
 @Bean(destroyMethod="close") public java.sql.Connection keeper(DataSource source)throws Exception {return source.getConnection();}
 @Bean public PlatformTransactionManager transactionManager(DataSource source){return new DataSourceTransactionManager(source);}
 @Bean public JobRepository jobRepository(DataSource source,PlatformTransactionManager manager)throws Exception {var factory=new JobRepositoryFactoryBean();factory.setDataSource(source);factory.setTransactionManager(manager);factory.afterPropertiesSet();return factory.getObject();}
 @Bean public JobExplorer jobExplorer(DataSource source,PlatformTransactionManager manager)throws Exception {var factory=new JobExplorerFactoryBean();factory.setDataSource(source);factory.setTransactionManager(manager);factory.afterPropertiesSet();return factory.getObject();}
 @Bean public TaskExecutorJobLauncher jobLauncher(JobRepository repository)throws Exception {var launcher=new TaskExecutorJobLauncher();launcher.setJobRepository(repository);launcher.afterPropertiesSet();return launcher;}
 @Bean public JobRegistry jobRegistry(Job fixture)throws Exception {var registry=new MapJobRegistry();registry.register(new ReferenceJobFactory(fixture));return registry;}
 @Bean public Job fixture(JobRepository repository,PlatformTransactionManager manager) {
  var step=new StepBuilder("counted",repository).tasklet((contribution,context)->{
   var data=context.getStepContext().getStepExecution().getExecutionContext();int count=data.getInt("count",0)+1;data.putInt("count",count);System.out.println("OPENIDEA_CONTROL_TICK="+count);Thread.sleep(200);return count>=40?RepeatStatus.FINISHED:RepeatStatus.CONTINUABLE;
  },manager).build();return new JobBuilder("fixture",repository).start(step).build();
 }
}
