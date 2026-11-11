package fixture;
import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Configuration;
import org.springframework.batch.core.Job;
import org.springframework.batch.core.Step;
import org.springframework.batch.core.job.builder.JobBuilder;
import org.springframework.batch.core.step.builder.StepBuilder;
import org.springframework.batch.core.repository.JobRepository;
import org.springframework.batch.core.repository.support.ResourcelessJobRepository;
import org.springframework.batch.core.launch.support.TaskExecutorJobLauncher;
import org.springframework.batch.support.transaction.ResourcelessTransactionManager;
import org.springframework.transaction.PlatformTransactionManager;
import org.springframework.batch.repeat.RepeatStatus;
@Configuration
public class BatchContext {
 @Bean public JobRepository jobRepository(){return new ResourcelessJobRepository();}
 @Bean public PlatformTransactionManager transactionManager(){return new ResourcelessTransactionManager();}
 @Bean public TaskExecutorJobLauncher jobLauncher(JobRepository repository) throws Exception {
  var launcher=new TaskExecutorJobLauncher();launcher.setJobRepository(repository);launcher.afterPropertiesSet();return launcher;
 }
 @Bean public Job fixture(JobRepository repository,PlatformTransactionManager transactionManager) {
  Step step=new StepBuilder("fixtureStep",repository).tasklet((contribution,context)->{System.out.println("OPENIDEA_BATCH_FIXTURE_COMPLETED");return RepeatStatus.FINISHED;},transactionManager).build();
  return new JobBuilder("fixture",repository).start(step).build();
 }
}
