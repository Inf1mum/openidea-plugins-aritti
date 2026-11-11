// expect: jpa-unknown-query-parameter (:wrong has no matching method parameter)
package com.example.shop.repository;

import com.example.shop.domain.Customer;
import java.util.List;
import org.springframework.data.jpa.repository.JpaRepository;
import org.springframework.data.jpa.repository.Query;

public interface UnknownParamRepository extends JpaRepository<Customer, Long> {

    @Query("select c from Customer c where c.name = :wrong")
    List<Customer> byName(String name);
}
