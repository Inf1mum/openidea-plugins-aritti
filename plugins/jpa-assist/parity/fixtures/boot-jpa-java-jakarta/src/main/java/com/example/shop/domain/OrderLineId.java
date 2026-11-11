package com.example.shop.domain;

import jakarta.persistence.Column;
import jakarta.persistence.Embeddable;
import java.io.Serializable;
import java.util.Objects;

@Embeddable
public class OrderLineId implements Serializable {

    @Column(name = "order_id")
    private Long orderId;

    @Column(name = "line_no")
    private Integer lineNo;

    protected OrderLineId() {
    }

    public OrderLineId(Long orderId, Integer lineNo) {
        this.orderId = orderId;
        this.lineNo = lineNo;
    }

    public Long getOrderId() {
        return orderId;
    }

    public Integer getLineNo() {
        return lineNo;
    }

    @Override
    public boolean equals(Object other) {
        if (this == other) {
            return true;
        }
        if (!(other instanceof OrderLineId that)) {
            return false;
        }
        return Objects.equals(orderId, that.orderId) && Objects.equals(lineNo, that.lineNo);
    }

    @Override
    public int hashCode() {
        return Objects.hash(orderId, lineNo);
    }
}
