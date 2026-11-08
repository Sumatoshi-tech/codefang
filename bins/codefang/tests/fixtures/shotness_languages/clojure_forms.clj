(defn- private-probe [x]
 (+ x 111))
(defmacro macro-probe [x]
 `(+ ~x 111))
(require '[clojure.test :refer [deftest is]])
(deftest test-probe
 (is (= 111 111)))
(defmulti method-probe identity)
(defmethod method-probe :default [x]
 (+ x 111))
