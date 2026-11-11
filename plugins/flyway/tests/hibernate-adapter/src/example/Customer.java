package example;
import jakarta.persistence.*;
@Entity @Table(name="customers", uniqueConstraints=@UniqueConstraint(name="uq_customer_email",columnNames="email"))
public class Customer { @Id @GeneratedValue(strategy=GenerationType.SEQUENCE) public Long id; @Column(nullable=false,length=80) public String name; @Convert(converter=EmailConverter.class) public String email; @ManyToOne @JoinColumn(name="department_id",nullable=false) public Department department; }
