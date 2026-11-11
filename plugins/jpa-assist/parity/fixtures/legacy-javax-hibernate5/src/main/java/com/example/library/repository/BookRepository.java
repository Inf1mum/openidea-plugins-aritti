package com.example.library.repository;

import com.example.library.domain.Book;
import java.util.List;
import java.util.Optional;
import org.springframework.data.domain.Page;
import org.springframework.data.domain.Pageable;
import org.springframework.data.jpa.repository.JpaRepository;
import org.springframework.data.jpa.repository.Query;
import org.springframework.data.repository.query.Param;

public interface BookRepository extends JpaRepository<Book, Long> {

    Optional<Book> findByIsbn(String isbn);

    Page<Book> findByTitleContaining(String fragment, Pageable pageable);

    @Query("select b from Book b join b.authors a where a.name = :name")
    List<Book> byAuthor(@Param("name") String name);
}
