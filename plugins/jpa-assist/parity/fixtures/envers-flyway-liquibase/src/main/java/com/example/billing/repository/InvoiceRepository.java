package com.example.billing.repository;

import com.example.billing.domain.Invoice;
import java.util.List;
import org.springframework.data.jpa.repository.JpaRepository;
import org.springframework.data.repository.history.RevisionRepository;

public interface InvoiceRepository extends JpaRepository<Invoice, Long>, RevisionRepository<Invoice, Long, Integer> {

    List<Invoice> findByNumber(String number);
}
