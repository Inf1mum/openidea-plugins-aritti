// expect: jpa-lombok-constructors (@Builder without a no-arg constructor)
package com.example.crm.defects;

import jakarta.persistence.Entity;
import jakarta.persistence.GeneratedValue;
import jakarta.persistence.Id;
import lombok.Builder;
import lombok.Getter;

@Entity
@Getter
@Builder
public class LombokBuilderDefect {

    @Id
    @GeneratedValue
    private Long id;

    private String name;
}
