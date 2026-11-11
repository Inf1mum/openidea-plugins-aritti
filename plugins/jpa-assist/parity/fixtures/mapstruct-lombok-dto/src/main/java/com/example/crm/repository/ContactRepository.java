package com.example.crm.repository;

import com.example.crm.domain.Contact;
import java.util.List;
import org.springframework.data.jpa.repository.JpaRepository;

public interface ContactRepository extends JpaRepository<Contact, Long> {

    List<Contact> findByAccountId(Long accountId);
}
