package com.example.shop.repository;

import com.example.shop.domain.Product;
import java.math.BigDecimal;
import java.util.List;
import org.springframework.data.jpa.repository.JpaRepository;

public interface ProductRepository extends JpaRepository<Product, Long> {

    List<Product> findByPriceLessThan(BigDecimal price);

    List<Product> findCheap(BigDecimal limit);
}
