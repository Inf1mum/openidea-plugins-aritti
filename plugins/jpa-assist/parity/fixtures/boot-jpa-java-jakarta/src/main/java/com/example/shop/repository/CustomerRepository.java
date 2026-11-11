package com.example.shop.repository;

import com.example.shop.domain.Customer;
import com.example.shop.dto.CustomerNameView;
import com.example.shop.dto.CustomerSummary;
import java.util.List;
import java.util.Optional;
import org.springframework.data.domain.Page;
import org.springframework.data.domain.Pageable;
import org.springframework.data.domain.Slice;
import org.springframework.data.jpa.repository.EntityGraph;
import org.springframework.data.jpa.repository.JpaRepository;
import org.springframework.data.jpa.repository.Query;
import org.springframework.data.repository.query.Param;

public interface CustomerRepository extends JpaRepository<Customer, Long> {

    Optional<Customer> findByEmail(String email);

    List<Customer> findByNameContainingIgnoreCaseOrderByNameAsc(String fragment);

    Page<Customer> findByAddressCity(String city, Pageable pageable);

    Slice<Customer> findByNameStartingWith(String prefix, Pageable pageable);

    boolean existsByEmail(String email);

    long countByAddressCity(String city);

    @EntityGraph("Customer.withOrders")
    Optional<Customer> findWithOrdersById(Long id);

    @EntityGraph(attributePaths = {"profile", "orders"})
    List<Customer> findByAddressPostalCode(String postalCode);

    @Query("select c from Customer c where c.email = :email and c.name = :name")
    Optional<Customer> lookup(@Param("email") String email, @Param("name") String name);

    @Query("select c.name as name, c.email as email from Customer c where c.id = :id")
    CustomerNameView projectNameById(@Param("id") Long id);

    @Query("select new com.example.shop.dto.CustomerSummary(c.id, c.name) from Customer c")
    List<CustomerSummary> summaries();

    @Query(value = "select * from customer where email ilike ?1", nativeQuery = true)
    List<Customer> searchNative(String pattern);
}
