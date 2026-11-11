package example;
import jakarta.persistence.*;
@Entity @Table(name="departments")
public class Department { @Id public Long id; @Column(nullable=false, length=60) public String title; }
