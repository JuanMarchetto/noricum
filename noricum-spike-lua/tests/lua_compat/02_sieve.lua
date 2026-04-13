-- Sieve of Eratosthenes — exercises tables, loops, integer arithmetic
local function sieve(n)
  local is_prime = {}
  for i = 2, n do is_prime[i] = true end
  for i = 2, math.floor(math.sqrt(n)) do
    if is_prime[i] then
      for j = i * i, n, i do is_prime[j] = false end
    end
  end
  local primes = {}
  for i = 2, n do
    if is_prime[i] then primes[#primes + 1] = i end
  end
  return primes
end
local p = sieve(200)
print(#p, "primes up to 200")
print(table.concat(p, ","))
