package example;
import jakarta.persistence.*;
import java.math.BigDecimal;
import java.time.LocalTime;
@Entity @Table(name="jpa32_records")
public class Jpa32Entity {
 @Id public Long id;
 @Column(precision=9,scale=2,check=@CheckConstraint(name="nonnegative_amount",constraint="amount >= 0")) public BigDecimal amount;
 @Column(secondPrecision=3) public LocalTime eventTime;
}
