# Fibers and channels with written types: generators, coroutines, loops over fibers, and tasks
# talking over `ev/chan`, `ev/give`, `ev/take` and `ev/select`.

(def Job :typedef {:id :number :payload :string})
(def Reply :typedef '(or [:done :number :number] [:failed :number :string]))

(defn countdown
  {:params [:number] :ret (fiber :number :nil)}
  "A fiber yielding n, n-1, ... 1."
  [n]
  (generate [i :down-to [n 1]] i))

(defn evens
  {:params [(fiber :number :nil)] :ret @[:number]}
  "The even values a fiber yields."
  [numbers]
  (def out @[])
  (each n numbers
    (when (even? n) (array/push out n)))
  out)

(defn fib-seq
  {:params [:number] :ret :fiber}
  "A coroutine yielding the first n Fibonacci numbers."
  [n]
  (coro
    (var a 0)
    (var b 1)
    (repeat n
      (yield a)
      (def next (+ a b))
      (set a b)
      (set b next))))

(defn take-all
  {:params [:fiber] :ret @[:any]}
  "Every value a fiber yields, resumed until it is done."
  [f]
  (def out @[])
  (while true
    (def v (resume f))
    (if (= :dead (fiber/status f))
      (break)
      (array/push out v)))
  out)

(defn run-job
  {:params [Job] :ret Reply}
  "Work a job; a payload of `bad` fails."
  [job]
  (if (= "bad" (job :payload))
    [:failed (job :id) "bad payload"]
    [:done (job :id) (length (job :payload))]))

(defn worker
  {:params [(channel Job) (channel Reply)] :ret :nil}
  "Take jobs until a nil arrives, giving a reply for each."
  [jobs replies]
  (forever
    (def job (ev/take jobs))
    (if (nil? job) (break))
    (ev/give replies (run-job job))))

(defn collect
  {:params [(channel Reply) :number] :ret @{:number :number}}
  "Sizes of the jobs done, by id, from the first n replies."
  [replies n]
  (def sizes @{})
  (repeat n
    (match (ev/take replies)
      [:done id size] (put sizes id size)
      [:failed id _] (put sizes id -1)))
  sizes)

(defn first-ready
  {:params [(channel :string) (channel :string)] :ret :string?}
  "Whatever of two channels has a message first."
  [a b]
  (match (ev/select a b)
    [:take _ msg] msg
    _ nil))

(assert (deep= @[4 2] (evens (countdown 5))))
(assert (deep= @[0 1 1 2 3] (take-all (fib-seq 5))))
(assert (= 15 (sum (countdown 5))))

(def jobs (ev/chan 4))
(def replies (ev/chan 4))
(ev/spawn (worker jobs replies))
(each [id payload] [[1 "abc"] [2 "bad"] [3 "de"]]
  (ev/give jobs {:id id :payload payload}))
(ev/give jobs nil)
(def sizes (collect replies 3))
(assert (= 3 (sizes 1)))
(assert (= -1 (sizes 2)))
(assert (= 2 (sizes 3)))

(def quiet (ev/chan 1))
(def loud (ev/chan 1))
(ev/give loud "hey")
(assert (= "hey" (first-ready quiet loud)))

(def squares
  {:type (fiber :number :nil)}
  (fiber/new (fn [] (each i (range 1 4) (yield (* i i))))))
(assert (deep= @[1 4 9] (seq [x :in squares] x)))
