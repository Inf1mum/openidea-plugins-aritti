package com.example.library.repository;

import com.example.library.domain.Author;
import java.util.List;
import java.util.Optional;
import org.springframework.data.jpa.repository.JpaRepository;
import org.springframework.data.jpa.repository.Query;
import org.springframework.data.repository.query.Param;

public interface AuthorRepository extends JpaRepository<Author, Long> {

    Optional<Author> findByName(String name);

    List<Author> findAllSorted();

    @Query("select a from Author a where a.country = :country")
    List<Author> inCountry(@Param("country") String country);
}
