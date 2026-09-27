class Cuidador(val nombre: String) {
    val animales: MutableList<Animal> = mutableListOf()

    fun adoptar(animal: Animal) {
        animales.add(animal)
    }

    fun ronda() {
        for (animal in animales) {
            println("$nombre cuida de ${animal.presentarse()}")
        }
    }
}
