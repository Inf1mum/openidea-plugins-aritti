package fixture;
import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Configuration;
import org.springframework.batch.core.job.Job;
import org.springframework.batch.core.step.Step;
import org.springframework.batch.core.job.builder.JobBuilder;
import org.springframework.batch.core.step.builder.StepBuilder;
import org.springframework.batch.core.repository.JobRepository;
import org.springframework.batch.core.repository.support.ResourcelessJobRepository;
import org.springframework.batch.core.launch.support.TaskExecutorJobOperator;
import org.springframework.batch.core.configuration.JobRegistry;
import org.springframework.batch.core.configuration.support.MapJobRegistry;
import org.springframework.batch.infrastructure.support.transaction.ResourcelessTransactionManager;
import org.springframework.transaction.PlatformTransactionManager;
import org.springframework.batch.infrastructure.repeat.RepeatStatus;
@Configuration
public class BatchContext {
 @Bean public JobRepository jobRepository(){return new ResourcelessJobRepository();}
 @Bean public PlatformTransactionManager transactionManager(){return new ResourcelessTransactionManager();}
 @Bean public JobRegistry jobRegistry() {return new MapJobRegistry();}
 @Bean public TaskExecutorJobOperator jobOperator(JobRepository repository,JobRegistry registry) throws Exception {var operator=new TaskExecutorJobOperator();operator.setJobRepository(repository);operator.setJobRegistry(registry);operator.afterPropertiesSet();return operator;}
 @Bean public Job fixture(JobRepository repository,PlatformTransactionManager transactionManager) {
  Step step=new StepBuilder("fixtureStep",repository).tasklet((contribution,context)->{System.out.println("OPENIDEA_BATCH_FIXTURE_COMPLETED");return RepeatStatus.FINISHED;},transactionManager).build();
  return new JobBuilder("fixture",repository).start(step).build();
 }
}
