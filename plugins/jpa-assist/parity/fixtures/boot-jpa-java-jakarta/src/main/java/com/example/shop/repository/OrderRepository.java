package com.example.shop.repository;

import com.example.shop.domain.CustomerOrder;
import com.example.shop.domain.OrderStatus;
import java.time.Instant;
import java.util.List;
import org.springframework.data.jpa.repository.JpaRepository;
import org.springframework.data.jpa.repository.Modifying;
import org.springframework.data.jpa.repository.Query;
import org.springframework.data.repository.query.Param;

public interface OrderRepository extends JpaRepository<CustomerOrder, Long> {

    List<CustomerOrder> findByStatus(OrderStatus status);

    List<CustomerOrder> findByCustomerEmailAndStatus(String email, OrderStatus status);

    List<CustomerOrder> findByCreatedAtAfter(Instant since);

    @Query("select o from CustomerOrder o join fetch o.lines where o.id = :id")
    CustomerOrder fetchWithLines(@Param("id") Long id);

    @Modifying
    @Query("update CustomerOrder o set o.status = :status where o.id = :id")
    int updateStatus(@Param("id") Long id, @Param("status") OrderStatus status);
}
