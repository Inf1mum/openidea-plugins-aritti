package com.example.shop.repository;

import com.example.shop.domain.OrderLine;
import com.example.shop.domain.OrderLineId;
import org.springframework.data.jpa.repository.JpaRepository;

public interface OrderLineRepository extends JpaRepository<OrderLine, OrderLineId> {
}
